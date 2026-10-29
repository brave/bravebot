---
id: XAI
title: Signing in to xAI with a SuperGrok subscription
status: proposed
governs:
  - crates/cli/src/auth.rs
  - crates/cli/src/xai.rs
  - crates/cli/src/doctor.rs
  - crates/config/src/lib.rs
  - crates/config/src/provider.rs
  - crates/aichat/src/models.rs
  - crates/agent/src/xai.rs
  - crates/tui/src/app/models.rs
  - crates/ui-bridge/src/models.rs
  - crates/agent/src/chatgpt.rs
  - crates/agent/src/backend.rs
  - crates/aichat/src/lib.rs
  - crates/ui-bridge/src/account_words.rs
  - ui/src/renderer/failure.ts
  - crates/skus/src/xai/mod.rs
  - crates/skus/src/xai/account.rs
  - crates/skus/src/xai/exchange.rs
  - crates/skus/src/xai/renew.rs
  - crates/skus/src/oauth/encoding.rs
  - crates/skus/src/oauth/file.rs
  - crates/skus/src/oauth/mod.rs
  - crates/skus/src/oauth/renew.rs
  - crates/skus/src/oauth/wire.rs
  - crates/skus/src/lib.rs
documented-by:
  - docs/website/docs/customize/providers/xai.md
  - docs/website/docs/customize/signing-in.md
  - docs/website/docs/reference/cli.md
---

## Scope

A person with a SuperGrok subscription signs in to xAI with a code. A program asks xAI's authorization
server for a code, shows the person an address and the code, and the person opens the address on any
device and enters the code. The program asks the authorization server whether the code has been used, and
stores the tokens it is given. This is the OAuth 2.0 device authorization grant against
`https://auth.x.ai/oauth2`. opencode's "SuperGrok Subscription" signs in this way.

This spec covers the command that signs in, the file the tokens are kept in, the replacing of an access token
before it expires, the models the sign-in may reach, the request a turn makes with it, what a turn does when
no sign-in is stored or the host does not accept the token, and how a failure is said.

The command is built: `auth login xai` signs in and stores the tokens, `auth logout xai` forgets them, and
`auth status` and the list of ways report them. A model named `xai/<name>` is then sent to the account, and
the account's models are offered where a person picks one. [What is built](#what-is-built) says where the
code is.

Reaching xAI with a pay-per-token API key is a different route, a gateway that [backends.md](backends.md)
describes. It needs none of this and this needs none of it.

The hosts, parameters, field names and error codes the clauses state are taken from opencode's source
(`packages/core/src/plugin/provider/xai.ts`, at commit `ac68d6a929`), which is another program's code, and
from xAI's discovery document (`https://auth.x.ai/.well-known/openid-configuration`). What was observed against
xAI's service, with one account on one day, is in [What has been observed](#what-has-been-observed), and what
was not is in [Not observed](#not-observed). What a person has to decide, and what reversing each decision
takes, is in [Decisions](#decisions).

The flow carries no workspace content and no model output. The rule that untrusted content never reaches the
driver or the planner is [AGENTS.md](../../agents/AGENTS.md), and [XAI-4](#XAI-4) checks this flow against
it. Where each part lives is [layering.md](layering.md). The sign-in that this one shares its file and its
renewal with is [chatgpt-sign-in.md](chatgpt-sign-in.md), and what a credential owes is
[credential-protection.md](credential-protection.md).

The flow:

1. The program asks `POST https://auth.x.ai/oauth2/device/code` for a code, giving a client, a scope and a
   referrer ([XAI-2](#XAI-2)). The reply gives a `device_code`, a `user_code`, a `verification_uri` and,
   optionally, `verification_uri_complete`, `expires_in` and `interval`.
2. The program shows the address and the `user_code` ([XAI-11](#XAI-11)).
3. The program asks `POST https://auth.x.ai/oauth2/token` whether the code has been used, at once and then
   after each wait, until the reply is tokens, a refusal, or the time the reply gave for the code has passed
   ([XAI-3](#XAI-3)).
4. The tokens are written to a file ([XAI-5](#XAI-5)). The access token is replaced, using the refresh token,
   before it expires ([XAI-6](#XAI-6)).

## Clauses

<a id="XAI-1"></a>
### XAI-1: a token reaches only the hosts compiled in for it

Three destinations are used, each a constant in the source:

- the endpoint that gives a code, `https://auth.x.ai/oauth2/device/code`;
- the token endpoint, `https://auth.x.ai/oauth2/token`, which is asked whether a code has been used and
  which takes the refresh;
- the serving host, `https://api.x.ai/v1`, where the account's models are asked for and answer
  ([XAI-13](#XAI-13)).

A page, a reply, a model, a settings file at any layer, an environment variable and a redirect name none of
them, and none is looked up from anything fetched, as [BACKEND-17](backends.md#BACKEND-17) holds for a
gateway's known name. A destination is routing, and routing is never derived from content
([ROUTE-2](routing.md#ROUTE-2)). xAI's discovery document lists the same two endpoints of the issuer and a
revocation endpoint. It is not fetched, and no request goes to the revocation endpoint.

A test points the flow at a server of its own through the seam [chatgpt-sign-in.md](chatgpt-sign-in.md)
states for the ChatGPT sign-in, with a variable of its own. In a build with the `test-hosts` feature of
`bravebot-skus` or of `bravebot-config`, debug assertions on and a panic that unwinds, the variable
`BRAVEBOT_TEST_XAI_STAND_IN` names one server that stands in for the three destinations, the serving host
being its address followed by `/v1`. The tests that hold that seam out of every build that ships hold this
variable too: no command, workflow or file of the repository names it, and a build without the feature holds
no code that reads it. The source a release compiles is read for who may name a host. The bodies of
`Hosts::compiled_in`, of `xai_serving_host` and of `Provider::xai_account` are compared to their text, so an
environment read outside the gate or a host that is not one of the three constants fails. The variable is a
literal in two gated constants, one in each crate, each read in one place, and the functions that take the
destinations of the sign-in are those of the sign-in's own module, by name. A second caller of any of these,
another reader of the variable, or another function that takes the destinations fails a test until this
clause says why.

The device code and the refresh token are sent to the token endpoint and to no other. The access token is sent
to the serving host and to no other, as a bearer on a request that follows no redirect ([XAI-16](#XAI-16)). The
request for a code carries no token. A request to either endpoint of the issuer follows no redirect either: a
redirect in answer to it is a failure, and nothing is sent to the address it names. No request to the issuer
carries an access token.

**Why.** A settings file may name a destination where the credential is one the person typed for that
destination. A checkout's settings file naming where a refresh token goes would send a person's subscription
to whoever wrote the file, and a token whose host came from a reply would go wherever the reply said. A
redirect re-sends the request, and with it the code or the token, to a host nobody compiled in.

`verified-by: bravebot_skus::xai::the_destinations_are_the_two_endpoints_of_the_issuer`
`verified-by: bravebot_config::provider::the_xai_account_is_reached_at_the_serving_host_and_is_given_no_key_of_its_own`
`verified-by: bravebot_agent::backend::the_xai_token_goes_to_the_serving_host_alone_and_a_redirect_goes_nowhere`
`verified-by: bravebot_cli::test_hosts_seam::the_xai_token_is_presented_through_one_client_given_the_provider_of_constants`
`verified-by: bravebot_skus::xai::a_redirect_in_answer_to_any_request_is_not_followed`
`verified-by: bravebot_skus::xai::renew::a_redirect_in_answer_to_a_refresh_is_not_followed_and_the_sign_in_is_left_alone`
`verified-by: bravebot_skus::xai::the_refresh_token_goes_to_the_token_endpoint_alone_and_no_request_carries_an_access_token`
`verified-by: bravebot_cli::test_hosts_seam::the_xai_hosts_are_three_constants_and_the_variable_that_names_a_stand_in_is_read_in_two_gated_places`
`verified-by: bravebot_cli::test_hosts_seam::the_command_reader_sees_a_seam_and_every_feature_however_a_command_spells_them`
`verified-by: bravebot_cli::test_hosts_seam::no_build_or_release_command_passes_a_seam_or_all_features_but_the_lint_runs_by_name`
`verified-by: bravebot_cli::test_hosts_seam::a_test_seam_is_enabled_by_dev_dependencies_alone`

<a id="XAI-2"></a>
### XAI-2: the request presents the client that opencode presents, and says it is bravebot

The request for a code presents these form fields, in this order, and no other:

| Field | Value |
|---|---|
| `client_id` | `b1a00492-073a-47ea-816f-4c329264a828` |
| `scope` | `openid profile email offline_access grok-cli:access api:access` |
| `referrer` | `bravebot` |

Every request to the token endpoint presents the same `client_id`. The requests are form encoded, and carry
`content-type: application/x-www-form-urlencoded`, `accept: application/json` and `user-agent: bravebot`. No
request carries a credential in a header.

The client is another program's. xAI's open-source Grok Build sets this `client_id` in the default OAuth2
provider configuration of `crates/codegen/xai-grok-login/src/config.rs` in its repository
(`github.com/xai-org/grok-build`, read on 2026-10-26), with the issuer `https://auth.x.ai`, and opencode
presents it. The maintainer decided that bravebot presents it, and stated that bravebot has permission to.
The discovery document, read on 2026-10-26, lists no `registration_endpoint`, and no route that registers a
client was found in it or in the xAI documentation that was read, so no client of bravebot's own is known.
The referrer names this program, where opencode's names itself and Grok Build's default is `grok-build`.

The ChatGPT sign-in presents no other program's client identifier, because OpenAI issues a client to each
program that registers one at its first sign-in. No such route was found here, and this clause is the decision
for this sign-in. [Decisions](#decisions) says what reversing it takes.

**Why.** The authorization server refused a request that names a client it does not hold: a client identifier
made up for the purpose was answered with a 400 and `invalid_client` on 2026-10-26. No route that registers
one for bravebot is known, and a sign-in that presented a placeholder would fail at its first request.

`verified-by: bravebot_skus::xai::a_sign_in_asks_for_a_code_under_its_client_and_polls_with_the_code_it_was_given`
`verified-by: bravebot_skus::xai::renew::a_refresh_presents_the_stored_refresh_token_under_the_client_and_writes_what_it_is_given`

<a id="XAI-3"></a>
### XAI-3: the address and the code are shown, the token endpoint is asked until the code is used, and the sign-in ends in one of a closed list

The reply for the code is shown to the person once, after it arrives and before the token endpoint is asked
anything. What is shown is the address, which is `verification_uri_complete` where the reply gives one and
`verification_uri` otherwise, and the `user_code`. An address is shown only if it is `https` and made of
printable ASCII characters, none of them a space, a code only if it is made of printable ASCII characters,
none of them a space, and neither is longer than 512 characters. A control character and a character outside
ASCII are not printable ASCII. A reply that gives one that is not is a reply that cannot be read. Among the
codes seen were `9XGF-VXWH`, `Y685-X42J`, `TD55-PTDB`, `TCNC-44W4` and `ZY9H-6RF7`, each two groups of four
uppercase letters and digits joined by a hyphen, which the rule accepts. A code of another shape that holds a
space would be refused as unreadable and shown to nobody ([Known costs](#known-costs)).

The token endpoint is asked at once and then after each wait. The wait is the `interval` the reply gave for
the code plus three seconds. The interval is five seconds where the reply gave none or one that is not more
than none, and a fraction of a second is one second. A reply whose error is `slow_down` adds five seconds to
the interval, for the wait that follows it and every wait after it. The code stops working `expires_in`
seconds after the reply for it, or 300 seconds after it where the reply gave none or one that is not more
than none. No wait passes that instant, so the sign-in ends there whatever interval the reply gave.

`expires_in` and `interval` are read as numbers. A value that is present and is not a number, such as text, a
list or a boolean, makes the reply one that cannot be read, and `null` or an absent field is none stated. The
lifetime is held to an hour and the interval to a minute, so a reply cannot hold the command for hours or make
it wait for longer than 63 seconds between asks, whatever it says and however often it asks to slow down. The
interval is never more than 60 seconds, so no wait is more than 63, and a code is never waited on for more than
3600 seconds. The service stated 1800 and 5 ([What has been observed](#what-has-been-observed)).

The lifetime a grant or a refresh reply states for an access token is held to between 300 seconds and 86400, so a
reply that states 18446744073709551615 does not make a token the clock never replaces, and one that states 1
does not make a refresh token be spent on every ask. A lifetime stated as 0 or less has run out, and is the
shortest, 300 seconds. The service stated 21600. This is the reading of this sign-in's replies alone and the
ChatGPT account's is in [CHATGPT-7](chatgpt-sign-in.md#CHATGPT-7). An access token whose reply states no
lifetime, or null, takes the `exp` claim of the token, held to the same two figures counted from the reply, so
an `exp` that never comes or that has passed does not defeat them either ([XAI-6](#XAI-6)).

An ask of the token endpoint ends in one of these, and no other:

- A success reply, which holds an `access_token` and a `refresh_token`. The sign-in is stored. A reply with no
  `refresh_token`, with one that is empty, or that cannot be read as a reply, ends the sign-in as a reply
  that cannot be read, since an access token alone cannot be renewed.
- A reply that is not a success, whose `error` is one of five codes. `authorization_pending` asks again after
  the wait. `slow_down` asks again after a longer one. `access_denied` ends the sign-in as access denied,
  `authorization_denied` as authorization denied, and `expired_token` as expired: three endings, each with a
  sentence of its own ([XAI-8](#XAI-8)).
- Any other reply that is not a success, whatever its `error` and whether it has one, ends the sign-in as the
  class of its status: rate-limited for 429, unavailable for 500 to 599, and refused for any other,
  a redirect included.

A request that did not complete ends the sign-in as a transport failure, with one exception. A poll of the token
endpoint whose request did not complete is asked again at the next interval, up to three polls in a row, since the
sign-in is waiting on a person to approve the code and a cut connection says nothing of the code. A poll that is
answered, with whatever reply, starts the count again, and a fourth poll in a row that does not complete ends the
sign-in as the transport failure it is. The request for a code, and a poll that fails in any other class, end it
at the first failure. The time passing ends it as timed out, whatever the polls did.
The request for a code failing ends it before anything is shown, as the class of its status. Nothing is stored
until tokens are granted, so a sign-in that ends otherwise leaves a stored one as it is, and one that
completes replaces it. The clock is read before anything is asked, at each ask and where the tokens arrive,
and a clock set before 1970 reads as no time, so a reading of none at any of them ends the sign-in with
nothing stored.

The requests of the sign-in and of a renewal are made on a connection of their own each, and none is kept for a
later one. A sign-in asks every eight seconds for up to an hour, which is up to about 450 new connections, and
the endpoint can take that. A connection kept between two polls is one the server or something in front of it
may have closed while the code waited for approval, which the next poll would meet as a transport failure. The
HTTP library probes a pooled connection before it reuses it, so this is a hardening against a connection closed
in a way that probe does not see. Whether a kept connection caused a transport failure seen in a real sign-in
was not isolated, and the effect of not keeping one on the real service was not observed. The agent is the one
both sign-ins' token endpoint requests use, so the ChatGPT sign-in's are made the same way.

**Why.** Each ending of the sign-in is a different thing for a person to do: nothing is wrong and the code is
waited on, xAI answered one of the two codes that deny the request, the code has to be asked for again, or the
service is unwell. The code is shown
before the first ask so that a person can use it while it is waited on. The three seconds added to the
interval keep the token endpoint from being asked more often than the reply said. Bounding every wait by the
time the code lasts keeps a reply from holding the command for longer than the code is good for, and the caps
keep a reply that states a day from holding it for one, since the figures come from the server's reply and are
not this program's own.

`verified-by: bravebot_skus::xai::the_address_and_the_code_are_shown_before_the_first_poll`
`verified-by: bravebot_skus::xai::an_address_with_no_code_in_it_is_the_one_shown_where_the_reply_gives_no_other`
`verified-by: bravebot_skus::xai::authorization_pending_is_asked_again_after_the_interval_and_three_seconds`
`verified-by: bravebot_skus::xai::a_code_that_gives_no_interval_is_asked_about_every_eight_seconds`
`verified-by: bravebot_skus::xai::slow_down_lengthens_the_wait_that_follows_it_and_every_one_after`
`verified-by: bravebot_skus::xai::access_denied_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing`
`verified-by: bravebot_skus::xai::authorization_denied_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing`
`verified-by: bravebot_skus::xai::expired_token_ends_the_sign_in_as_its_own_ending_after_one_ask_and_stores_nothing`
`verified-by: bravebot_skus::xai::a_code_nobody_uses_ends_the_sign_in_at_the_time_it_states`
`verified-by: bravebot_skus::xai::a_code_that_states_no_lifetime_lasts_five_minutes_whatever_the_interval`
`verified-by: bravebot_skus::xai::a_code_is_waited_on_for_an_hour_at_most_and_asked_about_every_minute_at_least`
`verified-by: bravebot_skus::xai::slow_down_never_lengthens_the_interval_past_a_minute`
`verified-by: bravebot_skus::xai::exchange::a_code_reply_that_states_more_than_the_caps_allow_is_held_to_them`
`verified-by: bravebot_skus::xai::exchange::a_lifetime_a_reply_states_is_taken_between_five_minutes_and_a_day`
`verified-by: bravebot_skus::xai::exchange::an_expiry_the_token_states_of_itself_is_held_to_five_minutes_and_a_day_from_the_reply`
`verified-by: bravebot_skus::xai::the_expiry_stored_is_the_lifetimes_then_the_tokens_claim_then_an_hour`
`verified-by: bravebot_skus::oauth::wire::two_requests_through_one_agent_do_not_share_a_connection_the_server_may_have_closed`
`verified-by: bravebot_skus::xai::up_to_three_polls_in_a_row_that_do_not_complete_are_asked_again_at_the_next_interval`
`verified-by: bravebot_skus::xai::a_poll_that_is_answered_starts_the_count_of_cut_polls_again`
`verified-by: bravebot_skus::xai::a_fourth_poll_in_a_row_that_does_not_complete_ends_the_sign_in_as_a_transport_failure`
`verified-by: bravebot_skus::xai::cut_polls_do_not_hold_a_sign_in_past_the_time_the_code_lasts`
`verified-by: bravebot_skus::xai::nothing_but_a_poll_that_does_not_complete_is_asked_again`
`verified-by: bravebot_skus::xai::any_other_failure_ends_the_sign_in_as_the_class_of_its_status_and_quotes_nothing`
`verified-by: bravebot_skus::xai::a_request_for_a_code_that_fails_shows_nothing_and_asks_the_token_endpoint_nothing`
`verified-by: bravebot_skus::xai::a_grant_that_holds_no_refresh_token_stores_nothing`
`verified-by: bravebot_skus::xai::the_expiry_stored_is_the_lifetimes_then_the_tokens_claim_then_an_hour`
`verified-by: bravebot_skus::xai::a_failed_sign_in_leaves_a_stored_one_and_a_completed_one_replaces_it`
`verified-by: bravebot_skus::xai::a_clock_set_before_1970_ends_the_sign_in_where_it_would_be_compared`
`verified-by: bravebot_skus::xai::exchange::a_code_reply_gives_what_is_shown_and_how_often_to_ask`
`verified-by: bravebot_skus::xai::exchange::a_code_with_no_lifetime_or_interval_lasts_five_minutes_and_is_asked_every_five_seconds`
`verified-by: bravebot_skus::xai::exchange::a_code_reply_that_is_not_the_recorded_shape_is_incomplete`
`verified-by: bravebot_skus::xai::exchange::what_is_shown_to_a_person_is_one_printable_word_and_an_address_is_https`
`verified-by: bravebot_skus::xai::exchange::a_token_reply_that_is_not_the_recorded_shape_is_incomplete`

<a id="XAI-4"></a>
### XAI-4: nothing the flow reads from outside reaches the driver or the planner

The sign-in runs as a command before any session, so no planner exists whose context it could enter. The
renewal of [XAI-6](#XAI-6) runs inside a turn, in the same crate, and its outcome reaches the agent: a token, or
a value of a closed set. Both read bytes this program did not write, which are the replies of the authorization
server. The crate compares and parses them and hands none of them to a turn, a trace or a message to the
planner. A turn's request adds the status of the serving host's reply and nothing else of it
([XAI-18](#XAI-18)). That is the argument
[chatgpt-sign-in.md](chatgpt-sign-in.md) makes for its own flow. It is checked against
[reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md), shape by shape:

- **A branch on untrusted bytes.** The comparisons are a reply's status, the fields the next step needs, the
  `error` of a reply that is not a success against the five codes of [XAI-3](#XAI-3), the `error` of a
  refused refresh against `invalid_grant` ([XAI-6](#XAI-6)), and the two properties of what is shown to the
  person, that it is printable and that an address is `https`. They decide this program's own sign-in and
  renewal. What leaves the crate is a token, the address and the code for the person's screen, a number of
  seconds, and a value of the closed sets `SignInError`, `TokenError`, `AccountError` and `Class`, each chosen
  from a step and the class of a status. The rest of a reply's body and its error text stay in the crate. In the
  agent, a turn's request adds one comparison: the status of the serving host's reply against 400, 401 and
  403, which decides whether the token is asked for once more and, after that, whether the failure is the
  refusal for the account ([XAI-18](#XAI-18)). The status is the number the
  transport read, from which every failure category is computed and on which a turn already branches, and
  the comparison does not read the body, which the request does not ask for. It decides neither where a
  request goes nor what it carries, and what a person is told is a sentence chosen from a closed value
  ([XAI-15](#XAI-15)). That is the ordinary case the document's section on the inverse mistake describes, and
  the footing the ChatGPT account's replacement of a token the host turned away with a 401 is on
  ([CHATGPT-7](chatgpt-sign-in.md#CHATGPT-7)).
- **The same branch moved into the kernel.** None is moved. The flow's code is in a crate that depends on
  neither `bravebot-core` nor `bravebot-agent`, which [layering.md](layering.md) treats as one driver, so no
  decision about a reply's content is taken in the driver.
- **A `declassify` outside the gates.** The flow has none, since it holds no labelled value.
- **A `Labelled` built by hand.** The flow builds none.

Three conditions keep the argument true, and a change that breaks one is a change to this clause:

1. The crate holds no `Labelled` and carries no workspace content and no model output, as `bravebot-skus`
   does.
2. What leaves the flow is a token in the secret type, a value from a closed set, a number of seconds and
   the address and the code. The address and the code are taken from a reply and handed to the function the
   caller gives for showing them, once, and to nothing else. No other string taken from a reply is part of
   anything that leaves the crate: not an error code, an error text or a field the flow does not read. What
   the agent receives from it is a token, a value of the closed set of [XAI-15](#XAI-15) chosen from the step
   that failed or the class of a status, and whether the file took a renewal, which is the result of a write on
   this machine. The body of the serving host's refusal is not asked for, so none of it is in a failure.
3. When the reply for an access token states no lifetime, the token's payload is decoded, if the token has
   three dot-separated parts and the second is base64url, and the `exp` claim is read. The signature is not
   checked, no other claim is read or kept, and the result decides when the program asks for another token
   and nothing else. A token that states a time that is not its own changes when this program asks for
   another, and reaches no decision about what is done with any content.

**Why.** The rule protects the planner and the driver from content an attacker could write. The one place
bytes from outside arrive here is a reply from the authorization server, over a connection to a host this
program compiled in, and what leaves the crate from it is a token, a category computed from a status, and a
code and an address the person reads. What a reply decides in a turn is the class of a failure and whether the
token is asked for once more, and neither chooses where a request goes or what it carries: the destination
is a constant, the request is the conversation the turn built, and the reply's text is not read.

`verified-by: bravebot_skus::xai::any_other_failure_ends_the_sign_in_as_the_class_of_its_status_and_quotes_nothing`
`verified-by: bravebot_skus::xai::a_request_for_a_code_that_fails_shows_nothing_and_asks_the_token_endpoint_nothing`
`verified-by: bravebot_skus::xai::exchange::what_is_shown_to_a_person_is_one_printable_word_and_an_address_is_https`
`verified-by: bravebot_skus::xai::exchange::the_expiry_is_the_replys_lifetime_then_the_tokens_claim_then_an_hour`
`verified-by: bravebot_skus::xai::account::a_refusal_of_a_malformed_file_quotes_none_of_it`
`verified-by: bravebot_cli::layering::a_rows_dependency_list_is_what_the_manifest_asks_for`

<a id="XAI-5"></a>
### XAI-5: the tokens are one file the account owns

The sign-in is kept in `xai-account.json` in the state directory. It holds a version, the access token, the
refresh token and the instant the access token expires, and nothing else: not the client, which is a constant
of the program, and not a field of a reply. The file is created before anything is written to it, reachable
by the account that signed in and by no other: mode 0600 on Unix and an access-control list for one
account on Windows, with nothing inherited. A platform with neither has the sign-in refused. It is written beside the file and renamed over it, so a write that stops
part way leaves the last good file. No settings layer, `provider` block or environment variable holds a token
or names one, and the file is not `settings.json`.

The file, and the claim `xai-account.lock` taken for every write and every removal of it, are this sign-in's
own. A claim the ChatGPT sign-in holds does not hold up a write of this file, and this
sign-in never writes another's file.

A file that cannot be read, is empty, or holds the fields of another version is reported with the remedy
and is not read as an absent sign-in.

A token is held in the secret type that [credential-protection.md](credential-protection.md) describes, so
it is redacted wherever it is printed, and its buffer is cleared when it is dropped. A program a turn starts
is refused the file and its claim: the run base reads the session records and no other file of the state
directory.

**Why.** A file of its own because the shape differs from the gateway keys, which are one string under an id
with no expiry, and from the ChatGPT account, which holds the client OpenAI issued. A claim of its own so that a write is held up only by another write of
this file, which a refresh holds across a request. A file that cannot be read is not read as absent, because
the next sign-in would then write over what a person may want to look at, and `auth status` would say "not
signed in" over a file that is there.

`verified-by: bravebot_skus::xai::account::a_stored_sign_in_is_read_back_field_for_field`
`verified-by: bravebot_skus::xai::account::the_file_holds_the_named_fields_and_nothing_else`
`verified-by: bravebot_skus::xai::account::no_file_is_no_sign_in`
`verified-by: bravebot_skus::xai::account::a_file_that_cannot_be_used_is_an_error_naming_its_defect_and_not_an_absent_sign_in`
`verified-by: bravebot_skus::xai::account::a_refusal_of_a_malformed_file_quotes_none_of_it`
`verified-by: bravebot_skus::xai::account::the_tokens_are_redacted_wherever_the_account_is_printed`
`verified-by: bravebot_skus::xai::account::the_file_is_readable_by_its_owner_alone_and_so_is_its_replacement`
`verified-by: bravebot_skus::xai::account::a_write_that_stops_part_way_leaves_the_last_good_file_and_no_temporary`
`verified-by: bravebot_skus::xai::account::a_second_sign_in_replaces_the_first_and_forgetting_removes_it`
`verified-by: bravebot_skus::xai::account::the_file_and_the_claim_are_this_sign_ins_own`
`verified-by: bravebot_agent::home::a_directory_named_in_the_environment_is_the_state_directory_in_every_crate`
`verified-by: bravebot_agent::home::a_value_that_names_no_directory_leaves_no_state_directory_to_guess`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_sandbox::base::the_run_base_reads_the_session_records_and_no_other_file_in_the_state_directory`

<a id="XAI-6"></a>
### XAI-6: an access token is replaced before it expires

An access token whose expiry is more than a minute away is handed out as it is. Otherwise the refresh token
is presented to the token endpoint with `grant_type=refresh_token`, the refresh token and the client, the
reply is written to the file, and only then is the new access token handed out. The expiry is the instant of
the reply plus its `expires_in` where the reply states one, otherwise the `exp` claim of the access token
([XAI-4](#XAI-4)), and otherwise the instant of the reply plus 3600 seconds, the first two held to between 300
seconds and 86400 from the reply ([XAI-3](#XAI-3)). The instant is read from the
system clock, and a clock set before 1970 reads as 0 here. A refresh reply that grants no refresh token keeps
the one presented. A reply that grants one replaces it.

The refresh is taken under an exclusive claim on the file, and the file is read again once the claim is held.
Every write of the file and its removal is made under the claim. A process that finds a token other than the
one it was about to replace uses it and presents nothing. A refresh request has a deadline of thirty seconds,
and a claim is judged abandoned once its file was last modified sixty seconds ago. A refresh makes one request,
or two where the first was refused as ended and the file had changed since (below), and the claim is made as
young as a new one before the second, so no request waits under a claim older than thirty seconds. A claim
older than sixty belongs to a process that died holding it, or to one that was stopped for longer than a
request may take, and is broken by moving it aside and judging it there. A holder that was stopped that long
can have its claim broken, and what follows from that is below.

A refresh the authorization server refuses as the refresh token no longer being valid ends the sign-in. That
is a status of 400 or 401 whose `error`, read as a string, is `invalid_grant`, which is RFC 6749 section
5.2's code for it. The file is read again immediately before it is removed, and is removed only while it holds
a refresh token this process knows to be spent: the one just refused, or the one it replaced before it. A
claim is judged abandoned by its age, so a holder that was stopped and is alive can have its claim broken, and
the process that broke it then presents a refresh token the holder has already spent and is refused for it
while the holder writes the tokens it was given. Where the file holds another refresh token the sign-in has not
ended: that token is used, and is replaced once with its own refresh token if it too is due, and a second
refusal with the file changed again removes nothing and is told as a refresh that failed. Where the file was
removed, every later ask of the same renewer says the sign-in has ended without asking the token endpoint
again, until a sign-in is stored. A refusal with any other code or status
is a failure of the request and ends nothing: the file is left as it is and the failure is one of the
classes of [XAI-3](#XAI-3). Where the refresh fails other than by ending the sign-in and the access token in
hand has not yet passed its expiry, that token is handed out. A refresh made after the refresh token was
revoked was answered with a 400 and `invalid_grant`, once, with one account
([What has been observed](#what-has-been-observed)).

Where a write of the reply fails, the new tokens are handed out and held in memory, the failure is handed
out with them, and the write is tried again under the claim before each later ask, only while the file still
holds the token that was replaced. Where the file holds another token or none, the tokens in memory are
dropped.

The renewal is the code that [chatgpt-sign-in.md](chatgpt-sign-in.md) describes for the ChatGPT account,
given this sign-in's file and request, so each state of it is exercised through that account's tests and
what is particular to this sign-in is exercised here. The re-read before a removal is in that shared code, so
it holds for both accounts, and the tests of the ChatGPT renewer pass under it. A turn asks it for the access
token before each request ([XAI-17](#XAI-17)).

**Why.** An authorization server may replace the refresh token each time it is used, and then the copy a
second process read before the first's reply is dead. The claim is held across up to two network round trips, so
its age of abandonment is twice a trip's deadline, and it is made young again between the two. The reply is written first because a process that stops between
receiving it and writing it has spent a refresh token it cannot present again. Removing the file on an ended
sign-in leaves `auth status` saying what is true. One refresh made with one account on 2026-10-26 was answered
with a refresh token that differed from the one presented ([What has been observed](#what-has-been-observed)),
so the claim and the early write are kept for a server that replaces the refresh token on use. Whether the
previous one still works after that was not tried, since a replay may end the sign-in, and the refresh token's
own lifetime was not observed. Users of other programs state that the refresh token is single use, which is
not confirmed ([Not observed](#not-observed)), and is the case the claim and the early write are kept for.

`verified-by: bravebot_skus::xai::renew::a_token_with_more_than_a_minute_left_is_used_and_nothing_is_asked`
`verified-by: bravebot_skus::xai::renew::a_refresh_presents_the_stored_refresh_token_under_the_client_and_writes_what_it_is_given`
`verified-by: bravebot_skus::xai::renew::a_reply_with_no_refresh_token_keeps_the_one_presented`
`verified-by: bravebot_skus::xai::renew::a_reply_with_no_lifetime_is_stored_with_the_tokens_own_expiry_or_an_hour`
`verified-by: bravebot_skus::xai::renew::a_refresh_refused_as_ended_removes_the_sign_in_and_any_other_leaves_it`
`verified-by: bravebot_skus::xai::renew::a_refusal_of_a_spent_refresh_token_does_not_remove_what_another_process_wrote`
`verified-by: bravebot_skus::xai::renew::a_sign_in_another_process_wrote_that_is_due_is_replaced_once_and_not_chased`
`verified-by: bravebot_skus::xai::renew::no_stored_sign_in_is_not_signed_in`
`verified-by: bravebot_skus::xai::renew::a_second_refresh_under_one_claim_is_made_with_the_claim_as_young_as_a_new_one`
`verified-by: bravebot_skus::oauth::wire::a_lifetime_is_seconds_from_the_reply_or_an_instant_held_between_two_distances`
`verified-by: bravebot_skus::chatgpt::renew::a_token_within_a_minute_of_its_expiry_or_past_it_is_replaced`
`verified-by: bravebot_skus::chatgpt::renew::a_renewal_under_a_clock_before_1970_stores_an_expiry_the_next_start_renews_again`
`verified-by: bravebot_skus::chatgpt::renew::a_renewal_made_while_waiting_for_the_claim_is_used_and_not_made_again`
`verified-by: bravebot_skus::chatgpt::renew::a_write_that_fails_is_reported_and_tried_again_before_the_next_request`
`verified-by: bravebot_skus::chatgpt::renew::a_failed_refresh_sends_a_token_that_has_not_expired_and_never_one_that_has`
`verified-by: bravebot_skus::chatgpt::renew::a_renewer_that_found_the_sign_in_end_says_so_to_every_later_ask`
`verified-by: bravebot_skus::chatgpt::renew::tokens_held_in_memory_are_dropped_where_the_file_holds_another`
`verified-by: bravebot_skus::chatgpt::renew::a_refusal_of_a_spent_refresh_token_does_not_remove_what_another_process_wrote`
`verified-by: bravebot_skus::chatgpt::renew::a_sign_in_another_process_wrote_that_is_due_is_replaced_once_and_not_chased`

<a id="XAI-7"></a>
### XAI-7: only a command a person types signs in

`bravebot auth login xai` is the way in, run by a person at a terminal. It is a sixth way on the list
`auth login` prints, and `bravebot auth logout xai` and `bravebot auth status xai` are its partners
([XAI-9](#XAI-9)). The way is named `xai` and takes no word after it. No tool the planner calls, no hook, no
settings file, no environment variable and no slash command in a session starts the flow, and a line that
arrives through a quarantined source and reads like the command is not the command. A session starts no
sign-in for it, as [BACKEND-9](backends.md#BACKEND-9) states for the ChatGPT account.

`xai` is also the id a gateway entry for an API key at the same host would have, which draft pull request
#3156 would make a known name ([BACKEND-17](backends.md#BACKEND-17)). The two are apart as `chatgpt` is from the
gateway id `openai` ([CHATGPT-1](chatgpt-sign-in.md#CHATGPT-1)): `bravebot auth login gateway xai` would store a
key for that gateway, and `auth status xai` and `auth logout xai` here name the sign-in alone.

The command ends, once the sign-in is stored, by saying what to do next, since a person who has just signed in
has no model name to give. Where a start would ask which of the account's models to use
([BACKEND-39](backends.md#BACKEND-39)) it sends the person to run `bravebot`, and also names a command that
gives a model, `bravebot -i "<prompt>" --model xai/<model>` or `bravebot "<task>" --model xai/<model>`. Where a
start would not ask about the account, because a model is in force that something serves, or because a Bedrock
or gateway block is configured and a start asks about that service's models instead, it names those commands and
promises no question about the account. It names `/model` only as what lists the
models once a session is open, since the sign-in opens none. It asks the service nothing to say this.

**Why.** The flow shows a code a person acts on, holds the terminal for as long as the code lasts, and ends
with a credential on disk. Each is a decision about a person's machine and account, and a person's own typing
is the one source for it that no content can author. A session in an editor, a desktop window or a background
run has nowhere to show the code.

`verified-by: bravebot_cli::layering::the_xai_sign_in_is_started_by_the_command_and_nothing_else`
`verified-by: bravebot_cli::layering::the_scan_for_the_xai_sign_in_finds_each_way_to_name_it_and_not_a_read_of_the_file`
`verified-by: bravebot_cli::auth::every_way_is_listed_and_only_a_held_one_says_what_it_holds`
`verified-by: bravebot_cli::auth::a_way_is_picked_by_its_number_or_its_name`
`verified-by: bravebot_cli::xai_sign_in::auth_login_xai_is_refused_before_a_code_is_asked_for_where_it_cannot_be_done`
`verified-by: bravebot_cli::xai_sign_in::a_sign_in_runs_end_to_end_and_no_value_it_handled_is_shown_or_kept_but_in_the_file`
`verified-by: bravebot_cli::xai_sign_in::the_xai_sign_in_command_says_what_to_run_next_and_promises_the_question_only_where_it_is_asked`

<a id="XAI-8"></a>
### XAI-8: a failure is one fixed sentence that quotes nothing

Every failure of the sign-in is one fixed sentence, chosen from the step that failed and, for a failed request,
the class of its status. None holds text taken from a reply or a request. The one for the sign-in file adds the
path and what the operating system said about it, which is text about a file and not text read from one, and
none holds text from a settings file. The sentences are these, each a different thing for a person to do next:

- **Access denied.** The token endpoint answered `access_denied`, so nothing was stored, and signing in again
  is the remedy.
- **Authorization denied.** It answered `authorization_denied`, with the same remedy in a sentence of its own.
  Neither sentence says why xAI answered as it did, since no reason was observed.
- **Expired.** The code stopped working before it was used. This is the answer `expired_token` gives.
- **Timed out.** The code was not used in the time this program waits for one: the time the reply gave for it,
  five minutes where it gave none, and never more than an hour ([XAI-3](#XAI-3)).
- **A request failed.** The request for a code, or a request to the token endpoint, was refused, was
  rate-limited, met an unavailable service, did not complete, or was answered in a shape that cannot be read.
  A grant with no refresh token is the last of these. The sentence says which, and says that nothing was
  stored.
- **The clock** is set before 1970, so the sign-in cannot tell when the code stops working.
- **The sign-in file** cannot be read, cannot be written, has nowhere to be kept on this platform, or is empty,
  not JSON, another version's or missing a field. The sentence names the remedy and prints the path and what
  the operating system said as one line, with any control character pictured.

A renewal that fails has sentences chosen from the same kind of value in `bravebot-agent`. A sign-in that was
never made and one that has ended, which is what a refresh refused as the refresh token no longer being valid
is, name `bravebot auth login xai`. A renewal that failed in another way names no command and says the
class of the failure, since the file is left as it is and the next ask tries again, and a file that cannot be
read names `bravebot auth status xai`. The terminal, the one-shot run, the window and the record of a session say
these where a turn renews ([XAI-15](#XAI-15), [XAI-17](#XAI-17)).

Each sentence is a message of the catalog, in English and in French, chosen from the value alone. The command
says the sentence as its error and ends with the failure status.

No device code, token, reply body or error text reaches an error message, a session record, the terminal or the
environment of a program a turn starts, and the command writes no log. The id token that the service sends with
the tokens holds the person's identity, and it is read for nothing and kept nowhere. The address and the code
the reply gave reach the terminal of the sign-in once ([XAI-11](#XAI-11)) and no file.

**Why.** An error text is a string a server chose. Said in a sentence, it would put that string in front of a
person and in a log for as long as the log lasts. The class of a status is a value this program computed, and
a sentence made of it says what happened without repeating anything.

`verified-by: bravebot_cli::xai::each_way_the_sign_in_stops_has_its_own_sentence_and_quotes_nothing`
`verified-by: bravebot_cli::xai::a_file_that_cannot_be_used_is_a_sentence_with_the_remedy_and_a_detail_that_is_one_line`
`verified-by: bravebot_cli::xai_sign_in::access_denied_ends_the_sign_in_with_its_own_sentence_in_both_catalogs`
`verified-by: bravebot_cli::xai_sign_in::authorization_denied_ends_the_sign_in_with_its_own_sentence_in_both_catalogs`
`verified-by: bravebot_cli::xai_sign_in::expired_token_ends_the_sign_in_with_its_own_sentence_in_both_catalogs`
`verified-by: bravebot_cli::xai_sign_in::a_request_for_a_code_that_is_refused_ends_the_sign_in_before_the_token_endpoint_is_asked`
`verified-by: bravebot_cli::xai_sign_in::a_sign_in_runs_end_to_end_and_no_value_it_handled_is_shown_or_kept_but_in_the_file`
`verified-by: bravebot_agent::xai::each_class_of_a_failed_request_has_its_own_sentence`
`verified-by: bravebot_agent::xai::each_reason_no_token_could_be_had_has_its_own_sentence_and_names_the_remedy_it_has`
`verified-by: bravebot_agent::xai::the_sentence_for_a_file_that_cannot_be_read_quotes_nothing_of_the_failure`
`verified-by: bravebot_skus::xai::any_other_failure_ends_the_sign_in_as_the_class_of_its_status_and_quotes_nothing`

<a id="XAI-9"></a>
### XAI-9: logging out and status never print the sign-in

`bravebot auth logout xai` removes `xai-account.json` and says that it ends nothing at xAI. It removes the copy
kept here and makes no request, so it does not call the `revocation_endpoint` that xAI's discovery document
lists, and the tokens are not ended by it. It is allowed in an incognito session and is not an error where
nothing is stored, as forgetting an import is.

What would end each token is stated here, and logging out here does neither. The
access token was issued for 21600 seconds, observed once with one account, and what the serving host answers a
token past that instant was not observed, so the instant is the one this program replaces it at and no more.
The refresh token ends when xAI revokes it: a revocation made on 2026-10-26 was answered with a 200, and a
refresh afterwards was refused as `invalid_grant` ([What has been observed](#what-has-been-observed)).

`bravebot auth status xai` prints one line. It says **signed in**, with the instant the access token expires,
where the file holds a sign-in, **not signed in**, with the command, where it does not, or **unusable**, with
the reason and the remedy, where the file cannot be read. An access token whose instant has passed is still
signed in, since the refresh token can replace it ([XAI-6](#XAI-6)). The signed in line names the two hosts a
token goes to: that the access token is sent to `api.x.ai` alone, which is the serving host ([XAI-16](#XAI-16)),
and that the refresh token is sent to `auth.x.ai` alone, which is the host of the token endpoint
([XAI-1](#XAI-1)). It names no other host. The command probes nothing and writes nothing. Its exit status is the one the other ways of that command give: 0
where the sign-in is usable. The line holds no token, claim, identifier, email or account name. With no way
named, `auth status` asks about this sign-in among the others.

The list `auth login` prints says on this way's line what `status` says where the file holds a sign-in, that
is **signed in** with the instant the access token expires and the two hosts the tokens are sent to, and says that
a stored sign-in cannot be read where it cannot. A list that said "signed in" alone would say less than the
command a person runs to find out. A word after the way is refused with the argument status,
for `logout` and for `status` as for `login`.

**Why.** Each is an answer about whether the next request will work, and none needs the secret to give it. An
identifier or an email on a status line is the one thing in a pasted report that says whose account it was.
Forgetting a sign-in that ended nothing at xAI is said as that, so nobody takes a removed file for a revoked
token.

`verified-by: bravebot_cli::xai_sign_in::auth_status_xai_reports_the_expiry_and_nothing_that_could_sign_a_request`
`verified-by: bravebot_cli::xai_sign_in::auth_logout_xai_forgets_the_sign_in_and_only_that_one`
`verified-by: bravebot_cli::xai_sign_in::the_list_of_ways_names_xai_and_says_signed_in_only_where_a_sign_in_is_stored`
`verified-by: bravebot_cli::xai_sign_in::a_sign_in_runs_end_to_end_and_no_value_it_handled_is_shown_or_kept_but_in_the_file`
`verified-by: bravebot_cli::xai::a_stored_sign_in_is_reported_by_its_expiry_alone_and_a_file_that_cannot_be_used_is_unusable`
`verified-by: bravebot_cli::xai::the_host_the_status_names_is_the_one_the_access_token_is_sent_to`

<a id="XAI-10"></a>
### XAI-10: signing in is refused where it cannot be done

`auth login xai` is refused in an incognito session before the code is asked for, as storing a gateway key
is refused there ([INCOG-7](incognito.md#INCOG-7)): the sign-in is a write to `~/.bravebot` that the mode exists to decline. It is refused with
the argument status where stderr is not a terminal, since the address and the code are shown there, and says
to run it at one. Where both apply, the incognito refusal is the one given. Neither refusal sends a request or
writes a file. The command asks nothing, so it needs no terminal on stdin.

**Why.** Refusing the sign-in is the argument [INCOG-7](incognito.md#INCOG-7) makes for every sign-in: doing it
and discarding the result would leave a session that was signed in once and never again. A code shown where nobody reads it is a code that
stops working unused.

`verified-by: bravebot_cli::xai::each_condition_that_cannot_be_answered_stops_the_command_with_its_own_ending`
`verified-by: bravebot_cli::xai_sign_in::auth_login_xai_is_refused_before_a_code_is_asked_for_where_it_cannot_be_done`

<a id="XAI-11"></a>
### XAI-11: the address and the code are always printed, so a machine with no browser can finish the sign-in elsewhere

The address and the code are written to stderr, line by line as they are written, once the reply for the
code has arrived and before the token endpoint is asked anything. There are four lines: what to do with the
address, the address, the code to enter if the page asks for one, and how long the code lasts, in whole
minutes rounded up. Each line is flushed as it is written. The command waits for the code to be used in every
case. It opens no browser and starts no program, so a person opens the address on any device, a phone included.

The address is the one that holds the code where the reply gives one, and the address the code is entered at
otherwise. Both are shown as the reply gave them, and only after the check of [XAI-3](#XAI-3).

**Why.** A sign-in that showed the address when it was over would show it after the code had stopped working.
A device-code flow is made for a person at a machine with no browser, so the command waits whether or not one
could be opened, and needs no port forwarded.

`verified-by: bravebot_cli::xai::the_address_and_the_code_are_shown_line_by_line_as_they_are_written`
`verified-by: bravebot_cli::xai::the_time_a_code_lasts_is_whole_minutes_rounded_up`
`verified-by: bravebot_cli::xai_sign_in::a_sign_in_runs_end_to_end_and_no_value_it_handled_is_shown_or_kept_but_in_the_file`
`verified-by: bravebot_skus::xai::the_address_and_the_code_are_shown_before_the_first_poll`

<a id="XAI-12"></a>
### XAI-12: each token is a credential at a tier, and doctor reports the sign-in

The sign-in is one entry in the record of held credentials that [credential-protection.md](credential-protection.md)
describes, for the pair of tokens, since the two are one arrangement ended at one surface.

- **Both are Held.** The refresh token mints access tokens without asking anyone, so a value this program
  renews without further authority is permanent whatever its stated lifetime. The walk is the three drops an
  AWS session has: nothing the agent cannot impersonate decides each use of the access token this program
  holds, nothing here asks xAI to narrow the sign-in to a run, and the refresh token replaces the access
  token without asking anybody. Each is a drop nobody attempted, since no narrower or bound form has been
  asked for.
- **What would end each is recorded, and it is not the same for the two.** The access token was issued for
  21600 seconds, observed once, and what the host answers one past that was not observed, so its expiry is taken
  to end that copy and no more. The refresh token ends when xAI revokes it,
  through the `revocation_endpoint` of its discovery document, `https://auth.x.ai/oauth2/revoke`, which this
  program does not call and which ended it once ([XAI-9](#XAI-9)). `bravebot auth logout xai` ends this
  machine's custody and ends nothing at xAI, and the record says so.
- **What revoking would not reach was seen once.** An access token already issued may keep working until it
  expires. One request made right after a revocation, with an access token issued before it, was accepted, once
  with one account ([What has been observed](#what-has-been-observed)). How long such a token keeps working was
  not observed, so the record says one may outlive it, and `doctor` says what was seen and no more.
- **Neither is bound to its presenter.** Nothing asks the authorization server for a sender-constrained form,
  so the record says bearer, with the answer `NotAttempted`.

`bravebot doctor` names the sign-in. Where the file holds one it prints an `xai` line with the instant the
access token expires, that the access token is sent to `api.x.ai` alone and that the refresh token is sent to
`auth.x.ai` alone ([XAI-9](#XAI-9)), then the account every held
credential is given: what would end it, the tier, the binding, what revoking would not reach, and the gate
each drop failed. A past instant is still signed in, since the refresh token can replace it. Where nothing
is stored it prints nothing about the sign-in, as it prints nothing for a gateway block that names no
credential. Where the file cannot be read it prints the line with the remedy and the run ends in failure, as
a file of gateway keys that cannot be read does, since the sign-in it held has stopped being used. The
sign-in is named where the configuration does not load, since the file is a fact about the machine and not
about the configuration. The report holds no token, and no field of the file but the instant.

**Why.** A record of what ends a credential is what a person who suspects a leak needs, and it can only be
written while the arrangement is understood. Held because the arrangement is the one that spec names as the
false pass, a short-lived token that its holder renews by itself. Saying that signing out ends nothing at xAI
keeps a person from taking a removed file for a revoked token.

`verified-by: bravebot_config::lib::an_xai_sign_in_is_held_renewed_unaided_and_possibly_outlives_revocation`
`verified-by: bravebot_cli::doctor::every_held_credential_has_its_own_account_of_what_would_end_it`
`verified-by: bravebot_cli::doctor::what_survives_revoking_is_reported_for_exactly_the_credentials_that_have_one`
`verified-by: bravebot_cli::doctor::a_drop_is_reported_with_the_gate_and_the_answer_the_record_holds`
`verified-by: bravebot_cli::doctor::every_drop_has_its_own_account_of_the_condition_it_failed`
`verified-by: bravebot_cli::xai_sign_in::doctor_names_an_xai_sign_in_by_its_expiry_and_prints_none_of_it`

<a id="XAI-13"></a>
### XAI-13: the account is named `xai`, its models are `xai/<name>`, and an entry with that id keeps the name

The account is the service at `https://api.x.ai/v1`, a constant of the source, spoken to in chat completions
as a gateway is. A model is the account's when its name is `xai/` and then the name xAI knows the model by.
The text after the identifier is what a request carries as the model. A name without the identifier never
selects the account, so a bare `grok-4.5` keeps its meaning: a gateway's model that lists it, or Brave's. The
identifier is the whole of the first segment and in lower case. `xai/` with nothing after it, `Xai/`, `x-ai/`
(which a gateway may use for a model of its own) and the identifier of another account do not select it.

A `provider` entry with the id `xai` is a gateway and keeps the name, as an entry with the id `chatgpt` does
([CHATGPT-9](chatgpt-sign-in.md#CHATGPT-9)). While one exists no model is the account's, in a request or in
a picker, and `bravebot auth login xai` is refused with the configuration ending, naming the block, before a
code is asked for, since one name cannot select two services. Only an entry that is a gateway holds the name:
one that states no endpoint and is not a known name configures no service ([BACKEND-17](backends.md#BACKEND-17)),
so it holds nothing, and its `models` are not read for the account's roster as an entry named `chatgpt` is.

No other entry takes the name. A gateway of another id that lists a model whose own name begins `xai/` keeps
that model, reached by the spelling its row has, `<id>/xai/<name>`, which sends the gateway `xai/<name>`. The
gateway's bare listing is not matched against a name that begins with the identifier, so `xai/<name>` selects
the account whatever a gateway lists. A Bedrock account does not take the name either: a block or a tier
variable that names a model `xai/<name>` is not asked about it, since a Bedrock model is reached by the id as
it stands and has no qualified spelling to be reached by instead.

The row `xai` that a draft change to the table of known names would add, for an API key at the same host,
turns an entry that names a key and no endpoint into a gateway. That entry would then hold the name, and a
person who has both an API key entry and a sign-in would be offered no account model and refused the sign-in
until the entry is renamed. Nothing in this clause or its tests is wrong for the endpoint-stating entry that
holds the name today, so what has to be decided when that row lands is whether one name may serve both, and a
test of an entry that names a key and no endpoint has to be added then.

**Why.** Where two services serve a model of one name they are billed differently, authenticate differently
and answer on different terms, so a bare name cannot say which one a person chose, and a remembered choice
would change service silently. An entry that already uses the id is not reinterpreted: refusing the sign-in
and naming the entry is a message, where taking the id over would send a working gateway's requests, and the
key it holds, to xAI's service with a person's subscription. A model a gateway lists under a name that begins
with the identifier is not a reason to send a signed-in person's turn to the gateway when the account's row of
that name was picked. The host is a constant for the reason a gateway's known names are: it is where a bearer
credential goes, and nothing a settings file or a reply says may move it.

`verified-by: bravebot_config::lib::only_the_qualified_name_selects_the_xai_account_and_only_the_rest_of_it_is_sent`
`verified-by: bravebot_config::lib::a_provider_entry_named_xai_keeps_the_name_and_no_model_is_the_accounts`
`verified-by: bravebot_config::lib::an_entry_named_xai_that_states_no_endpoint_holds_nothing_and_its_models_are_not_read`
`verified-by: bravebot_config::lib::a_gateway_listing_a_name_that_begins_with_xai_is_reached_by_its_qualified_name_alone`
`verified-by: bravebot_config::lib::a_bedrock_account_listing_a_name_that_begins_with_xai_does_not_take_it_from_the_account`
`verified-by: bravebot_config::provider::the_xai_account_is_reached_at_the_serving_host_and_is_given_no_key_of_its_own`
`verified-by: bravebot_cli::xai::each_condition_that_cannot_be_answered_stops_the_command_with_its_own_ending`
`verified-by: bravebot_cli::xai_sign_in::auth_login_xai_is_refused_where_a_gateway_holds_the_name`

<a id="XAI-14"></a>
### XAI-14: the account's models are asked for at the serving host, and a written rule leaves out the ones the chat route refuses

The roster is the serving host's listing: `GET https://api.x.ai/v1/models` with the access token as a bearer
and nothing else. No other credential goes with it, and the route a gateway answers for what one key may reach
is not asked. A redirect in answer is a failure, and nothing is sent to the address it names
([XAI-1](#XAI-1)). The reply is an object whose `data` key holds the rows, and a body of another shape is one
that cannot be read. A refusal is told by its status alone: its body is not asked for, so nothing of it is
read or kept.

A row is offered when its `id` is a string that is `grok-` and then a digit or `build-`, and holds none of
`multi-agent`, `imagine`, `image` and `video`. The rule reads the id and nothing else, because no field of the
listing separates the models the chat route answers from the ones it refuses: the multi-agent model has a window,
as the text models have, and so do the image rows. A row in another form costs that row its place and keeps the
rest: an `id` that is not a string, or a row that is not an object at all, such as `null`, a string, a number or
a list. The rows are read as values for that reason, and a listing is unreadable only where its `data` is not a
list. One row is offered for a name. Rows are offered in the order the service listed them. Nothing is capped.

Each row is `xai/<id>` ([XAI-13](#XAI-13)), shown by its id, filed under `xAI`, and not a premium row, since the
subscription is not Leo's. Its window is the row's `context_length` where that is a whole number of more than
none, and otherwise the assumed low window a gateway's model gets, which is the figure a turn compacts against. No
row reads an effort level: the service was not asked with one ([XAI-16](#XAI-16)), and the listing's
`reasoning_effort` capability, which only four of the rows state, is not read. A listing that is read and has no
such row offers nothing. One that cannot be read offers nothing, and no list built into this program stands in
for it, since a name on a list promises that picking it works and the only evidence of what an account may reach
is its own listing.

What the rule keeps was observed once. The listing an account with a SuperGrok sign-in was given on 2026-10-26
held fourteen ids. Seven answered a request on the chat route: `grok-4.20-0309-non-reasoning`,
`grok-4.20-0309-reasoning`, `grok-4.3`, `grok-4.5`, `grok-4.6`, `grok-4.7` and `grok-build-0.1`. The route
answered the other seven with a 400: `grok-4.20-multi-agent-0309` and six ids that begin `grok-imagine-`. The rule
keeps the seven and drops the seven. A model that is not offered is still reached by naming it, `xai/<id>`.

**Why.** The listing is the one source that says what an account may reach on a date, and a list that waited for
a release would offer a model the account cannot reach for weeks. The listing is also content: it decides what
a picker draws, and the person's pick is the endorsement for the field it lands in, as for a gateway
([BACKEND-19](backends.md#BACKEND-19)). What may not come from a reply is where a request goes, and that is a
constant ([XAI-1](#XAI-1)). An id rule is written in the source, so a reply cannot make it offer a row, only
fail to match one. It is a rule that can be wrong in both directions, and each costs something different: an id
that passes and is refused at the first request is refused as any request is, and an id the rule drops is not
offered, such as an image model that begins as a text model does. The second is the cheaper to bear, since naming
the model reaches it. No built-in list stands in for an
unreadable listing, because one account's listing on one day does not show what another account may reach.

`verified-by: bravebot_aichat::models::the_rule_keeps_the_seven_chat_models_of_the_observed_listing_and_drops_the_other_seven`
`verified-by: bravebot_aichat::models::an_id_is_kept_where_it_begins_as_a_text_model_does_and_holds_no_word_of_another_kind`
`verified-by: bravebot_aichat::models::a_window_the_xai_listing_states_is_taken_and_a_missing_or_malformed_one_is_the_assumed_one`
`verified-by: bravebot_aichat::models::an_xai_row_in_an_unexpected_form_is_dropped_alone_and_a_name_listed_twice_is_one_row`
`verified-by: bravebot_aichat::models::a_row_that_is_not_an_object_costs_that_row_and_not_the_listing`
`verified-by: bravebot_aichat::models::no_row_of_the_xai_account_reads_an_effort_level_whatever_its_capabilities_say`
`verified-by: bravebot_aichat::client::the_xai_listing_is_asked_for_at_the_serving_host_with_the_token_and_nothing_else`
`verified-by: bravebot_aichat::client::a_redirect_in_answer_to_the_xai_listing_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_aichat::client::a_reply_that_is_not_the_xai_listing_cannot_be_read_and_an_empty_one_can`

<a id="XAI-15"></a>
### XAI-15: a failure of the account is one fixed sentence, the same in every front end

A failure of a turn's request on the account is one of ten values: nothing is stored, the sign-in has ended, the
serving host refused the access token and a new one as well ([XAI-18](#XAI-18)), the serving host refused the
request for this account ([XAI-18](#XAI-18)), the sign-in file cannot be read, and a renewal that failed as one
of the five classes of a request that [XAI-3](#XAI-3) lists. Each is a
fixed sentence of the message catalog, in English and in French, chosen from the value alone. None holds text
taken from a reply, a request, a file or a settings file: the service answers a token it does not accept with a
403 or a 400, and an error body can carry an identifier, so the body of a refusal is not read.

The sentence names the command that is the remedy. `bravebot auth login xai` is named for a sign-in that is not
stored, one that has ended and a token that was refused, and `bravebot auth status xai` for a file that cannot be
read. A renewal that failed names none, since the file is left as it is and the next request asks again. The
refusal for the account names none either: its sentence says that xAI refused the request for this account, that
signing in again will not change that, and that a person can check the plan at xAI or choose another model.

The sentence is the same wherever a person is told of the failed turn: the terminal's line, the one-shot run's
report, the line for a goal check or a summary that could not be made, the line for a move to the fallback
model, and the record of the session. The category beside it is `unauthorized` for nothing stored, a sign-in that has ended, a token that was
refused and a file that cannot be read, `refused` for the refusal for the account, and the class of the request for
a renewal, so a fallback model answers for one that was rate-limited or unavailable and for no other. The window
has a sentence of its own for each, in sentence case, one sentence, sent with the command it names so that the
window draws the command as code and reads no sentence to find it, and it is sent beside a fixed name of the
value, `xai-` and then the step. An editor reached over the Agent Client Protocol is told the same sentence,
since the bridge sets it for any failure of an account ([ACP-8](acp.md#ACP-8)). The card the window draws has
a title that names neither service, since the sentence under it names one, and one card serves both accounts
because the window keeps no table of service names.

**Why.** The words are chosen from a closed value for the reason [XAI-8](#XAI-8) gives for the sign-in's own:
a string a server chose, said in a sentence, would put that string in front of a person and in a record for as
long as the record lasts. The two services share the value that carries a failure to the front ends, and have
sentences, commands and names of their own, so that a front end given an xAI failure cannot say ChatGPT's
sentence and send a person to sign in to the wrong service. A card titled for one service would do the same.

`verified-by: bravebot_agent::xai::each_failure_of_a_turns_request_has_its_own_sentence_and_names_the_remedy_it_has`
`verified-by: bravebot_cli::xai_sign_in::a_request_refused_with_a_403_renews_the_token_once_and_only_a_second_403_is_the_accounts_refusal`
`verified-by: bravebot_agent::xai::each_failed_step_of_a_renewal_is_its_own_failure_and_only_an_ended_sign_in_is_ended`
`verified-by: bravebot_agent::backend::an_xai_failure_is_diagnosed_by_its_step_and_names_the_command_where_it_is_the_remedy`
`verified-by: bravebot_agent::report::a_failure_of_the_xai_account_is_told_in_its_own_sentence_in_a_summary_and_a_fallback`
`verified-by: bravebot_session::sessions::a_failure_of_the_account_is_said_in_the_sentence_of_its_own_step`
`verified-by: bravebot_ui_bridge::account_words::the_xai_sentences_name_xais_command_where_it_is_the_remedy_and_never_chatgpts`
`verified-by: bravebot_ui_bridge::account_words::the_windows_sentence_is_one_sentence_in_sentence_case_and_names_its_command_once`
`verified-by: bravebot_ui_bridge::failure_tests::a_failure_of_the_xai_account_is_sent_as_its_own_name_with_xais_sentence_and_command`
`verified-by: bravebot_ui_bridge::xai_secrets::what_the_bridge_sends_and_keeps_of_a_failed_xai_turn_holds_none_of_the_values_of_the_run`
`verified-by: bravebot_tui::app::a_compact_that_failed_on_the_xai_account_says_its_own_sentence`
`verified-by: bravebot_ui_bridge::acp::a_turn_that_failed_on_the_xai_account_tells_the_editor_which_command_signs_in`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/marking.test.mjs draws the card for a failure of the xAI account and holds that its title names neither service, that the sentence is one paragraph with the command drawn as code, and that the category's advice about settings is not said beside it; make check-ui runs it)`

<a id="XAI-16"></a>
### XAI-16: a request on an account model is made in chat completions to the serving host, with the access token as a bearer and no effort level

A turn whose model is `xai/<name>` ([XAI-13](#XAI-13)) is sent to the account: a chat completions request, made
by the client every gateway is sent by ([BACKEND-17](backends.md#BACKEND-17)), to
`https://api.x.ai/v1/chat/completions`, with the access token as a bearer in `authorization` and no other
credential. The model it names is the part after the identifier. The conversation, the tools, the streaming and
the output ceiling are the request's own and are written and read as a gateway's are, and what else a gateway's
request carries is as it is for a gateway: a prefix the service may cache is marked, and a request the service
refuses for the mark is sent again without it, once, as [BACKEND-22](backends.md#BACKEND-22) states. The name the
reply reports is not compared with the name asked for: the listing states aliases for its models
(`grok-code-fast-1` for `grok-build-0.1`), a name given as one is taken, and whether the service answers an alias
under its canonical id was not observed. A run that worked is not ended as a model that was not served. The
exemption is for every `xai/` name and not for the aliases alone ([Known costs](#known-costs)).

No effort level is sent. The listing states `reasoning_effort` levels for four of its seven chat models, in a
field [BACKEND-22](backends.md#BACKEND-22) does not read for a gateway, and the service was not asked with a
level. A level the session chose stays chosen and applies to every other service, and the account is
reported as reading none, so no front end says a level is in force that is not sent. That holds for every name
of the account, whether or not the listing has a row for it: a one-shot or plain run that chose a level on a model
the listing offers, on an alias, on an id the rule leaves out, on one nobody listed, or where the listing could
not be read, says the model reads no effort level and sends none.

The access token goes to the serving host and to no other, and the refresh token to the token endpoint and to
no other ([XAI-1](#XAI-1), [ROUTE-2](routing.md#ROUTE-2)). The client is built with the account's provider, which
is made from constants and holds no variable, and it follows no redirect: a redirect in answer is a failure with
its status, and neither the token nor the conversation is sent to the address it names. A `provider` entry
with the id `xai` ends the account ([XAI-13](#XAI-13)), one with another id is a gateway of its own whose key is
sent to its own host, and a variable and a settings file name no destination for the token. So a gateway's key
never goes to xAI, and the account's token never goes to a gateway or to Brave's endpoint.

**Why.** The service took chat completions as other gateways do, and it accepted the token as a bearer
there, on whole requests of 16 or 32 tokens with no tools, for seven models ([What has been
observed](#what-has-been-observed)). So no second client, protocol or websocket is needed: the request, its
streaming and its tools are the code that every gateway turn already runs, and streaming and tool calls were
not tried. It also took the Responses protocol, on one model and for one tiny request, which opencode uses
over a websocket, and [Decisions](#decisions) says why this program does not. The level is not sent because a level asked of a model that does not read one is a charge
somebody chose and did not get ([BACKEND-22](backends.md#BACKEND-22)), and nothing here knows which models read
one: the service was not asked, and the listing states the levels in another field. Refusing a redirect keeps
a request that carries a conversation and a credential from being sent to a place a reply named.

`verified-by: bravebot_agent::backend::an_xai_model_selects_the_account_and_a_bare_name_selects_nothing_of_it`
`verified-by: bravebot_agent::backend::a_gateway_entry_named_xai_keeps_its_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::backend::a_gateway_listing_an_xai_name_does_not_take_it_from_the_account`
`verified-by: bravebot_agent::backend::a_request_on_an_xai_model_is_made_in_chat_completions_with_the_bearer_and_no_level`
`verified-by: bravebot_agent::backend::the_xai_token_goes_to_the_serving_host_alone_and_a_redirect_goes_nowhere`
`verified-by: bravebot_agent::backend::the_xai_account_reads_no_level_is_one_service_and_is_asked_for_the_name_after_its_identifier`
`verified-by: bravebot_agent::xai::the_request_the_account_is_sent_carries_no_level_and_is_otherwise_the_one_built`
`verified-by: bravebot_aichat::client::a_gateway_client_refusing_redirects_goes_nowhere_a_reply_names`
`verified-by: bravebot_cli::test_hosts_seam::the_xai_token_is_presented_through_one_client_given_the_provider_of_constants`
`verified-by: bravebot_cli::test_hosts_seam::only_the_agent_names_the_host_the_account_is_sent_to_and_no_release_calls_the_seam`
`verified-by: bravebot_cli::xai_sign_in::a_turn_on_an_xai_model_is_sent_to_the_account_with_the_stored_token_and_nothing_else`
`verified-by: bravebot_agent::backend::an_xai_alias_is_not_expected_to_be_the_name_that_comes_back`
`verified-by: bravebot_cli::xai_sign_in::a_run_on_an_alias_answered_under_its_canonical_id_is_not_a_failed_run`
`verified-by: bravebot_cli::xai_sign_in::a_level_chosen_for_a_run_on_any_name_of_the_account_is_said_to_be_unread_and_is_not_sent`
`verified-by: bravebot_tui::app::a_run_on_any_name_of_the_xai_account_is_told_the_model_reads_no_level`
`verified-by: bravebot_agent::backend::a_stop_reaches_a_request_on_the_xai_account_and_does_not_wait_for_the_host`
`verified-by: bravebot_agent::backend::the_stop_a_backend_is_given_is_held_by_the_xai_account_variant`

<a id="XAI-17"></a>
### XAI-17: the token is asked for before each request, a turn with no usable sign-in ends before one, and a token about to expire is replaced inside the turn

The access token is asked for before each request, by the renewer of [XAI-6](#XAI-6), which is made once for the
process. A turn on an account model that finds no usable sign-in ends before a request is sent: the failure is the
step that found it, which is nothing stored, a sign-in that has ended, or a file that cannot be read
([XAI-15](#XAI-15)), and its sentence names the command ([BACKEND-9](backends.md#BACKEND-9)). No turn, hook, tool or
slash command starts a sign-in: `bravebot auth login xai` is typed by a person ([XAI-7](#XAI-7)), and the layering
test holds that no other place names the flow.

An access token within a minute of its expiry is replaced inside the turn, before the request that would
present it, with the refresh token, and the reply is written to the file before the new token is used. A refresh
the authorization server refuses as the refresh token no longer being valid ends the turn with the sentence
that says the sign-in has ended and names the command, and removes the file. A renewal that failed as a class
of request ends the turn with that class's sentence, which names no command, and leaves the file as it was. Where
the renewal succeeded and the file could not take it, the tokens are used from memory for the rest of the
process, and the turn that made the renewal hands the front end a notice once, in a sentence that says the
sign-in lasts only until the program exits and what to check, as [CHATGPT-7](chatgpt-sign-in.md#CHATGPT-7) does
for ChatGPT's. A front end that shows a turn's notices says it where the turn was made, which the terminal
interface does. A one-shot or plain run that ends well prints none, because its progress keeps a notice only for
a turn that ends in failure, as for the ChatGPT account, so the next start finds the file's refresh token spent
and says the sign-in has ended ([Known costs](#known-costs)). xAI answered a refresh with a refresh token that
differed from the one presented, once with one account, so a renewal the file could not take may leave a file
whose refresh token no longer works. Whether the previous one still works was not tried.

**Why.** A token read once for a session would be past its expiry in a long turn, which is 21600 seconds after
the sign-in on the one observation ([What has been observed](#what-has-been-observed)). A request sent first and
the failure read from its answer would send a conversation to a service that is going to refuse it for no
credential. A sign-in begun from inside a turn would show a code where nobody is reading it and end with a
credential on disk that nobody chose to write ([XAI-7](#XAI-7)).

`verified-by: bravebot_agent::backend::a_turn_on_an_xai_model_with_no_usable_sign_in_ends_before_a_request_naming_the_command`
`verified-by: bravebot_agent::backend::the_token_is_asked_for_at_each_request_and_not_once_for_a_session`
`verified-by: bravebot_agent::backend::a_token_that_cannot_be_replaced_ends_the_turn_in_the_renewals_sentence_with_no_second_request`
`verified-by: bravebot_agent::xai::no_request_is_made_where_the_sign_in_has_no_token_to_give`
`verified-by: bravebot_agent::xai_renewal_notice::a_turn_says_an_xai_renewal_the_file_could_not_take_where_it_was_made_and_once`
`verified-by: bravebot_agent::xai::a_token_whose_renewal_the_file_could_not_take_is_noted_and_said_once_where_it_is_handed_out`
`verified-by: bravebot_cli::layering::the_xai_sign_in_is_started_by_the_command_and_nothing_else`
`verified-by: bravebot_cli::xai_sign_in::a_turn_on_an_xai_model_with_no_sign_in_ends_naming_the_command_and_asks_xai_nothing`
`verified-by: bravebot_cli::xai_sign_in::an_access_token_about_to_expire_is_replaced_inside_the_turn_and_the_new_one_is_sent`
`verified-by: bravebot_cli::xai_sign_in::a_refresh_xai_refuses_as_ended_ends_the_turn_naming_the_command_and_quotes_nothing`
`verified-by: bravebot_cli::xai_sign_in::a_renewal_that_fails_as_a_class_ends_the_turn_with_no_command_and_keeps_the_sign_in`

<a id="XAI-18"></a>
### XAI-18: a request the host refuses with a 400, a 401 or a 403 is made again once with a new token, and what is refused after that is decided from statuses alone

The service answered a bearer that is no token at all with a 400 and `invalid-argument`, and an access token it
could not validate, one altered in its signature or its payload, with a 403 and `unauthenticated:bad-credentials`
(on one account and at `GET /models` alone, [What has been observed](#what-has-been-observed)). A 401 is the
status that is conventional for both. A 400 is
also what a request that is wrong in another way is answered with: the service answered a model the chat route
does not take with a 400, and a prompt that is too long for the model is a 400 at other gateways, which was not
observed at xAI. The reply's text is not read to tell any of them apart, so two 403s that mean different things
are not told apart either. The driver may not branch on bytes a server sent, and an error body carried the
account's team identifier, which nothing may repeat. So the decision is made from the status alone, as follows:

1. A request that fails with a 400, a 401 or a 403 is made again once, after the access token is replaced
   ([XAI-6](#XAI-6)). The replacement is asked for once for the request, and never again for it.
2. Where the token could not be replaced, the turn ends with the renewal's failure ([XAI-15](#XAI-15)): a sign-in
   that has ended names the command, and a renewal that failed as a class of request does not. That is so for a
   first 403 as for a first 400 or 401.
3. Where the new token is accepted, the turn goes on, and nothing is said of the first.
4. Where the new token is refused with a 401, the sign-in is refused and the turn ends with the sentence that
   says so and names `bravebot auth login xai`. A token the authorization server has just issued was refused,
   and no other status says as much.
5. Where the new token is refused with a 403, the turn ends with the refusal for this account. A token the
   authorization server has just issued is not one the host could not validate, so this 403 is not the one an
   invalid token gets. The sentence says that xAI refused the request for this account, that signing in again
   will not change that, and that the plan can be checked at xAI or another model chosen. It names no command
   and quotes nothing of the reply, and the category is `refused`, which no fallback model answers for.
6. Where it is refused with any other status, including a 400, the request was the fault and not the token. The
   failure is the status as it is for any gateway, so a prompt that is too long is still a refusal of the body
   that shortens the conversation ([COMPACT-14](compaction.md#COMPACT-14)), and no sign-in is said to have been
   refused.

A failure that is none of the three statuses, such as a model the account does not have, a redirect, a rate
limit, a failure of the service or a stop, asks the authorization server for nothing. A turn that makes two
requests reports both in the count of requests it spent. A listing of the account's models is asked as a turn's
request is: refused with a 400, a 401 or a 403 it is asked for again once with a new token, and refused again it
offers nothing for the account ([XAI-19](#XAI-19)).

**What the account's refusal rests on.** The 403 of step 5 is the one a request gets that a renewal did not mend.
Bravebot has not observed it: the account this was built against was accepted, and an account on a plan that
cannot use this sign-in was not observed (tracked by brave/bravebot#3377). The sentence of step 5 for a
403 after a renewal is therefore a reading of a status that is not confirmed. A 403 that outlives a renewal for
another reason is told the same way, and a person told to check the plan would be told something that may not be
so.

Other parties report refusals of signed-in accounts, in programs that are not bravebot, and those reports are
the whole of the evidence for step 5 ([Not observed](#not-observed) lists them). They state a 403 for
`GET /models` and `POST /chat/completions` on a standard SuperGrok account (Hermes Agent issue 26847), and the
same refusal text with a 402 on an account with an active SuperGrok sign-in (broccoli-core issue 23). A 402
falls under step 6 and is told as the status it is. The reports disagree about whether the cause is the plan or a
quota, so they do not confirm that a person told to check the plan is told the cause.

A 403 on a token refresh is the authorization server's, and is told as a refresh that failed
([XAI-3](#XAI-3)), not by that sentence. The sentence says only that the request was refused for this account,
that signing in again will not change it, and that a person can check the plan at xAI or choose another model.

**Why.** Concluding from a 400 that the sign-in was refused would tell a person to sign in again for every
request the service dislikes, and a person who did would have no change. Reading the text would let whatever
wrote it decide what this program does next. Renewing and sending once more tests the one thing a status cannot
say, whether the token was the cause, with an answer from the authorization server that is itself a status: a
renewal that succeeds and is refused again with a 400 proves the token was not the problem, and one the
authorization server refuses as invalid proves the sign-in is over. The rule is for statuses the service was
seen to use. It was seen to answer an access token it could not validate with a 403, so a first 403 is a question
about the token and is answered by a renewal. It was not seen to answer a 401 to a token, so that status is read as
the conventional one and may have another cause. A 403 that outlives a renewal is told differently from a 401
because the token is then a fresh one, so it is not an invalid token, and a sentence that sends a person to sign
in again would send them to what a fresh token did not mend.

**Costs.** A request the service answers with a 400, a 401 or a 403 spends a refresh and costs at least one more
request before it reaches the turn, whether the token was at fault or not. A refusal for the account, if the
service makes one, costs the same before it is told, and was not observed. The client sends a request refused with
a 400 again once without the marks of its prefix, with the token it had ([BACKEND-22](backends.md#BACKEND-22)), so
the refused token is presented twice before it is replaced, and the replacement is then presented once or twice
as well. The refresh token is replaced on each refresh. Every 400 does this, including the 400 the service gave
for a model id the rule of [XAI-14](#XAI-14) leaves out and a person named, and a turn whose requests are refused
repeatedly for another reason spends a refresh for each. Where the authorization server is rate-limiting, a
failure to renew replaces the status the request failed with in what the person is told, so a prompt that is too
long is not shortened, which [COMPACT-14](compaction.md#COMPACT-14) would do on the 400.

`verified-by: bravebot_agent::backend::a_request_refused_with_a_400_a_401_or_a_403_is_made_once_more_with_a_replacement_token`
`verified-by: bravebot_agent::backend::a_replacement_refused_with_a_401_is_the_sign_in_refused_with_a_403_the_account_and_with_a_400_a_bad_request`
`verified-by: bravebot_agent::backend::a_request_refused_with_a_403_renews_the_token_once_and_a_second_403_is_a_refusal_for_the_account`
`verified-by: bravebot_agent::backend::a_token_that_cannot_be_replaced_ends_the_turn_in_the_renewals_sentence_with_no_second_request`
`verified-by: bravebot_agent::backend::a_failure_that_is_not_a_400_a_401_or_a_403_asks_the_authorization_server_for_nothing`
`verified-by: bravebot_agent::xai::only_a_400_a_401_or_a_403_asks_for_a_replacement_token`
`verified-by: bravebot_agent::xai::a_second_refusal_is_the_sign_in_with_a_401_the_account_with_a_403_and_the_request_with_a_400`
`verified-by: bravebot_agent::xai::a_listing_refused_with_a_403_twice_replaces_the_token_once_and_offers_nothing`
`verified-by: bravebot_cli::xai_sign_in::a_request_refused_with_a_403_renews_the_token_once_and_only_a_second_403_is_the_accounts_refusal`
`verified-by: bravebot_cli::xai_sign_in::a_request_refused_with_a_400_is_made_again_with_a_new_token_and_the_turn_goes_on`
`verified-by: bravebot_cli::xai_sign_in::a_new_token_refused_with_a_401_is_the_sign_in_refused_and_one_refused_with_a_400_is_not`

<a id="XAI-19"></a>
### XAI-19: the account's models are offered by the picker of the terminal and of the window, and are usable by name

Once a usable sign-in is stored, the models of [XAI-14](#XAI-14) appear in `/model` and in the desktop picker as a
roster beside the others, under the provider `xAI`, each named `xai/<name>` ([XAI-13](#XAI-13)). They take no model
away from another roster, and without a sign-in none is offered and the service is asked nothing. A model named `xai/<name>` in `--model` or in the `model` key is sent to the
account ([XAI-16](#XAI-16)) whether or not the listing offered it.

One function asks for the roster, and both pickers call it, so the terminal and the window offer the same rows. It
asks the renewer for the token before anything is sent ([XAI-17](#XAI-17)), asks for the listing ([XAI-14](#XAI-14)),
and replaces the token once where the host refuses the listing with a 400, a 401 or a 403 ([XAI-18](#XAI-18)). A sign-in
that was never made or has ended offers nothing and says nothing. A stored sign-in whose listing could not be had,
because the file cannot be read, the token could not be renewed, the request failed or was refused, or the reply is
not a listing, offers nothing for the account, since no list is built in, and the picker says so: the terminal in a
note, written when the picker opens, that names `bravebot auth login xai` and the two commands that name a model, and
the window in a warning that names the sign-in command alone, which the window draws as code, since a window has no
command line. A listing that was read and is empty offers nothing and says nothing, since it is the service's answer.
A `provider` entry named `xai` that is a gateway ends the account's rows ([XAI-13](#XAI-13)), and the service is
asked nothing.

The roster is asked each time a picker opens, and otherwise only for the window of the model in force where that is
the account's, so a session on another model, a one-shot run on another model and a turn on another model send the
stored token nowhere. The rule that says which accounts a roster asks is one function of the configuration,
`Config::roster_accounts_for_a_turn`, with `RosterAccounts::for_a_picker` for a picker, and the terminal and the window
both take it. A row carries the window the listing states, and reads no effort level ([XAI-16](#XAI-16)), so
neither front end says a level is in force on it.

A start that nothing serves asks the same function for the rows to choose from, as [BACKEND-39](backends.md#BACKEND-39)
states for both accounts: the rows it gives, in the picker's order and without the models the machine-level layer
refuses, and where it gives none the listing that gave none, so the start can say why. A listing that could not be
had is no row there, as it is none in the picker. A start asks it only where the xAI sign-in is stored, and it
sends the token to the serving host for the listing as any picker does.

The desktop picker draws the rows it is sent and groups them by the provider each names. Nothing in it names a service,
so the account's rows reach it as they are: the failure card is the one place the window names a service, and
[XAI-15](#XAI-15) says what it shows there.

The desktop picker opens before its listing arrives. The menu is drawn, its search field is focused and a line says
the models are loading, and the only row is the model already in force. What is typed meanwhile narrows the rows when
they arrive, and Enter chooses nothing until they have. The listing is one request answered when every endpoint has
answered, so the rows arrive together.

**Why.** None is offered without a sign-in, for the reason [XAI-14](#XAI-14) gives for offering only what the listing
shows, and a roster that could not be had is said where it is missed and not guessed at from one account's listing
on one day. Both pickers ask
one function so they cannot disagree about which rows exist or when the account is asked, and the token goes only for
a turn on one of the account's models, which is the one place a person has agreed it should go.

`verified-by: bravebot_agent::xai::a_machine_nobody_signed_in_on_offers_nothing_and_asks_the_service_nothing`
`verified-by: bravebot_agent::xai::a_signed_in_account_is_offered_what_the_service_lists_under_its_own_identifier`
`verified-by: bravebot_agent::xai::a_listing_refused_for_the_token_is_asked_again_once_with_a_replacement`
`verified-by: bravebot_agent::xai::a_listing_that_cannot_be_had_offers_nothing_and_is_told_apart_from_an_empty_one`
`verified-by: bravebot_agent::xai::a_gateway_entry_named_xai_keeps_the_name_and_the_account_is_not_asked_about`
`verified-by: bravebot_config::lib::a_roster_asks_the_account_a_turn_runs_on_and_no_other_and_a_picker_asks_both`
`verified-by: bravebot_tui::app::the_picker_asks_the_xai_account_whatever_model_is_in_force_and_lists_what_it_offers`
`verified-by: bravebot_tui::app::a_caller_asks_the_xai_account_only_for_one_of_its_own_models_and_takes_the_window_it_lists`
`verified-by: bravebot_tui::app::the_picker_says_why_the_xai_accounts_models_are_missing_and_says_nothing_otherwise`
`verified-by: bravebot_tui::app::a_start_is_offered_what_the_xai_account_listed_in_the_pickers_order_and_nothing_else`
`verified-by: bravebot_ui_bridge::models::the_xai_accounts_models_join_the_roster_and_a_failed_listing_adds_none_and_a_warning`
`verified-by: bravebot_ui_bridge::models::a_turn_in_the_window_asks_the_xai_account_only_for_one_of_its_own_models`
`verified-by: bravebot_ui_bridge::models::the_models_list_asks_the_xai_account_whatever_model_is_configured`
`verified-by: bravebot_ui_bridge::account_words::the_xai_pickers_warning_is_one_sentence_naming_the_sign_in_and_nothing_is_said_where_nothing_failed`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/marking.test.mjs draws the picker's warnings and holds that the command a warning names is drawn as code and the rest as text, and ui/scripts/drive-models.mjs opens the real picker against a model list held back, to hold that the menu, the loading line and the focused search field are drawn before the list arrives, that no row but the model in force is drawn until it does, and that the words typed meanwhile narrow the rows that arrive, and then against a model list with one warning and the command it names; the rows are drawn from the fields the bridge sends, grouped by the provider they name, in ui/src/renderer/components/ModelPicker.tsx, which holds no service's name; make check-ui runs them)`

## What is built

The sign-in is the `xai` module of `bravebot-skus`, and the command is `crates/cli/src/xai.rs`. The file, the
claim it is written under, the requests to a token endpoint and the renewer are the `oauth` module of the same
crate, which the ChatGPT sign-in uses as well. The xAI sign-in states which file, what it holds and what a
refresh presents (`Xai`, in `crates/skus/src/xai/account.rs`), and `Renewer` is that renewer for it.
[LAYER-1](layering.md#LAYER-1)'s row for `bravebot-skus` covers both modules, which depend on no crate of this
workspace, so [XAI-4](#XAI-4)'s first condition holds by reading its manifest. The account is `Backend::XaiAccount`
in `crates/agent/src/backend.rs`, which builds the client every gateway uses with the provider that
`Provider::xai_account` makes from constants, in `crates/config/src/provider.rs`. What a turn does with the token
is `crates/agent/src/xai.rs`: the renewer of the process, the rule of [XAI-18](#XAI-18), the roster both pickers ask
and the sentences for a failure, which the front ends say by the value `AccountFailure::Xai` carries to them.
The listing and the rule on its ids are `crates/aichat/src/models.rs`.

Built whole: [XAI-1](#XAI-1) to [XAI-19](#XAI-19).

## Decisions

These are a person's to decide, and no test can settle them.

- **The client.** The sign-in presents the client identifier opencode presents, and the maintainer stated
  that bravebot has permission to ([XAI-2](#XAI-2)). The alternative is a client xAI issues for bravebot, and no
  route that registers one was found. Reversing the decision takes three changes: `auth login xai` refuses or
  leaves the list of ways, the constant is deleted, and the tests that name it follow. `auth logout xai` stays,
  since tokens already stored are for a client nobody presents and a person can still forget them.
- **The referrer.** The request names `bravebot`. opencode sends `opencode`, and sending another program's name
  would attribute this program's sign-ins to it. Changing the name changes one constant.
- **Signing out does not call the revocation endpoint.** `auth logout xai` removes the copy kept here and makes
  no request ([XAI-9](#XAI-9)), as opencode does. The alternative is to ask xAI's `revocation_endpoint` to end
  the refresh token first. It works with the public client: on 2026-10-26 it answered a 200, a refresh
  afterwards was refused as `invalid_grant`, and an access token issued before it was accepted on a request made
  right after ([What has been observed](#what-has-been-observed)). Calling it adds a destination to
  [XAI-1](#XAI-1), a request that logout makes, and a failure of that request for logout to word.
- **Two codes that deny the request and one that expires the code are three sentences.** `access_denied`,
  `authorization_denied` and `expired_token` each end the sign-in with a sentence of their own
  ([XAI-8](#XAI-8)). The two denials are worded apart by the code that was answered and not by a reason, since
  none was observed ([Not observed](#not-observed)). The alternative is one sentence for both, which tells a
  person nothing the second sentence does not.
- **A completed sign-in replaces a stored one without asking.** [XAI-10](#XAI-10) asks nothing. The alternative
  is the question the ChatGPT sign-in asks, which needs a terminal on stdin. A sign-in that fails leaves the
  stored one as it is.
- **A refresh refused as `invalid_grant` ends the sign-in** and removes the file ([XAI-6](#XAI-6)). The
  alternative is to leave the file and fail each later refresh until the person signs in again. Removing it
  follows RFC 6749 and the ChatGPT sign-in, and this server answered a refresh of a revoked refresh token in
  that code once ([What has been observed](#what-has-been-observed)).
- **Chat completions, not Responses.** A turn on the account is sent in chat completions ([XAI-16](#XAI-16)),
  as every gateway is. The service also took the Responses protocol on one model, and opencode uses it over a
  websocket as its default. This program has no websocket client and the chat route was observed to take the
  token, so using it adds no protocol. The alternative is the Responses client that the ChatGPT account uses
  ([BACKEND-61](backends.md#BACKEND-61)), over HTTP, which was tried for one model. Reversing the decision takes
  one new variant of the backend that builds that client with this account's provider and token, and a test of
  each model it is sent.
- **A refused sign-in is decided from statuses alone, and a 400 is never concluded to be the sign-in.** A
  request refused with a 400, a 401 or a 403 is made once more with a new token, and the sign-in is refused
  only where a new token is refused with a 401 ([XAI-18](#XAI-18)). The alternatives are to treat a 400 as
  the sign-in refused, which tells a person to sign in again for every request the service dislikes, to read the
  reply's text, which the rule forbids the driver and which carried an identifier, and to ask the authorization
  server before every request, which is a round trip to answer a question the next request answers. The rule's
  cost is a refresh and a request for each such status. The driver branches on a status here, as it does for
  every failure category, and a reviewer reads that against [reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md).
  Reversing it takes the statuses of the rule and the arm of `Backend` that applies it.
- **A 403 is renewed once like a 400 or a 401, and only a 403 after the renewal is told as a refusal for the
  account.** The host answered an access token it could not validate with a 403, so a first 403 is a question
  about the token, and the turn asks for a new token and sends the request again before it concludes anything
  ([XAI-18](#XAI-18)). The alternatives are to end the turn at the first 403 with the sentence for the account,
  which tells a person whose token was merely invalid that their plan is the problem, and to treat a second 403
  as a 401, which tells a person to sign in again for what a fresh token did not mend. The sentence for the
  account after a renewal rests on a refusal that bravebot has not observed (a plan that cannot use this sign-in
  was not tried, brave/bravebot#3377) and that other programs' users report with a 403
  ([XAI-18](#XAI-18)), and a 403 for another reason is told the same way. The cost is a refresh and a
  request for a refusal that is the account's. Reversing it takes the statuses of
  `MAY_BE_THE_TOKEN` and the `NOT_PERMITTED` arm of `presenting` in `crates/agent/src/xai.rs`, the value
  `XaiFailure::NotPermitted` and its two sentences.
- **The roster is the listing filtered by a rule on the id, with no list built in.** [XAI-14](#XAI-14) keeps the
  id of a text model and drops the ones the chat route refused. A listing that cannot be read offers nothing. The
  alternatives are the list of seven ids, which is one account's listing on one day, and no filter, which would
  offer seven rows that fail when picked. Changing the rule changes a constant and its test.
- **No effort level is sent.** The service was not asked with one and the listing states the levels in a field
  [BACKEND-22](backends.md#BACKEND-22) does not read ([XAI-16](#XAI-16)). The alternative is to read
  `capabilities.reasoning_effort` as that clause reads a gateway's `supported_parameters`, and send a level where
  the list is not empty, which also needs the levels this program sends mapped to the ones the list names.
- **The account's requests follow no redirect.** The serving host is a constant and carries a token and a
  conversation, as the ChatGPT account's is ([XAI-16](#XAI-16)). A configured gateway follows them, since its
  host is the person's own.
- **A start with a stored sign-in asks which of the account's models to use, over the mechanism of the
  ChatGPT account's start.** [BACKEND-39](backends.md#BACKEND-39) states it for both accounts: the xAI file is
  read where the ChatGPT file is, the refusal names xAI as a route and says an xAI sign-in is stored where it is,
  and the question lists the rows of each stored account that lists some. The alternative is to leave the start
  unasked and the refusal naming no xAI route, and have the person name a model with `--model xai/<model>` or
  the `model` key, which asks about ChatGPT's models alone and leaves a person who has signed in to xAI and
  nothing else at a refusal that does not mention the sign-in. Reversing it takes removing the `xai` field of
  `Serving::NothingConfigured` and the xAI arm of the question in `crates/cli/src/model.rs`.
- **Up to three polls in a row that do not complete are asked again, and no connection is kept between
  requests.** A kept connection may be closed while the code waits, and the next poll would meet that as a
  transport failure ([XAI-3](#XAI-3)). The HTTP library probes a pooled connection before it reuses it, so not
  keeping connections is a hardening against a close the probe does not see. Whether a kept connection caused a
  transport failure seen in a real sign-in was not isolated, and the effect on the real service was not
  observed. Three cut polls in a row are also tolerated, so a poll that the network drops does not end a
  sign-in that is waiting on a person. The alternative is to end at the first, which is simpler and ends a
  sign-in for every dropped packet. The cost is up to three more intervals before a machine that cannot
  reach the service is told so: 24 seconds on the default interval where the connection fails at once, and up to
  114 seconds where it hangs, since each of the three polls then waits out the 30-second deadline of a request
  before the next interval. A new connection for each poll is up to about 450 an hour. Reversing it takes the
  guard on one `match` arm in `crates/skus/src/xai/mod.rs` and its constant.
- **A code is waited on for an hour at most and asked about at least every 63 seconds.** The caps are in
  [XAI-3](#XAI-3), and the service stated 1800 seconds and 5. The alternative is to take the reply's figures as
  stated, which lets a reply hold the command for as long as it says. Changing a cap changes one constant.

## What has been observed

Observed on 2026-10-26 against xAI's service, with one real SuperGrok account (the `tier` claim of its access
token was 3), signed in through the flow this spec states with a `referrer` of `bravebot`. One account on one
day: each statement is something observed once, and not a guarantee for another plan or a later day. Only
shapes were recorded, and no token, subject, email or name.

- A request for a code (`POST /oauth2/device/code`) was answered with a 200 and `application/json` four times,
  holding `device_code`, `expires_in` (1800), `interval` (5), `user_code`, `verification_uri`
  (`https://accounts.x.ai/oauth2/device`) and `verification_uri_complete` (the same address with the code in
  `user_code`). The `user_code` values seen included `9XGF-VXWH`, `Y685-X42J`, `TD55-PTDB`, `TCNC-44W4` and
  `ZY9H-6RF7`, each two groups of four uppercase letters and digits joined by a hyphen. The referrer `bravebot`
  was accepted. A client identifier made up for the purpose was answered with a 400 and `invalid_client`.
- The token endpoint, asked under the device grant, answered a 400 with `authorization_pending` while the code
  was unused: eight asks, eight seconds apart, and the ninth, after the code was approved in a browser, a 200.
  A code nobody used answered a 400 with `expired_token` after the 1800 seconds, once. Two other codes ended
  at the probe's own deadline before the service answered.
- The granted reply held `access_token`, `expires_in` (21600), `id_token`, `refresh_token`, `scope` (as asked)
  and `token_type` (`Bearer`). The access token is a JWT whose `exp` minus `iat` was 21600 seconds, and the
  refresh token is opaque. The id token is a JWT with identity claims, which is personal data. This program
  keeps the two tokens and an expiry, and reads nothing else of a reply ([XAI-4](#XAI-4)).
- A refresh (`grant_type=refresh_token`, the refresh token and the client) was answered with a 200 holding
  `access_token`, `expires_in` (21600), `refresh_token`, `scope` and `token_type`, and no `id_token`. The refresh
  token in the reply differed from the one presented, and the access token changed.
- A revocation (`POST /oauth2/revoke`, with the token, `token_type_hint=refresh_token` and the client) was
  answered with a 200 and an empty body. A refresh afterwards was answered with a 400 and `invalid_grant`. The
  access token issued before the revocation was accepted by `https://api.x.ai/v1` on a request made right after
  it. This program does not call the revocation endpoint.
- The serving host, `https://api.x.ai/v1`, accepted the access token as `Authorization: Bearer` over HTTPS on
  `GET /models`, `POST /chat/completions` and `POST /responses`, with no websocket and no header beyond a
  user agent and `accept`.
  - `GET /models` answered a 200 with `data` and `object`, and fourteen ids, which [XAI-14](#XAI-14) lists. Each
    row held `id`, `object` (`model`), `owned_by` (`xai`), `created`, `aliases` and price fields. The text
    models held `context_length`: 256000 for `grok-build-0.1`, 500000 for `grok-4.5`, `grok-4.6` and `grok-4.7`,
    and 1000000 for `grok-4.3`, `grok-4.20-0309-non-reasoning`, `grok-4.20-0309-reasoning` and
    `grok-4.20-multi-agent-0309`. The image rows held 16000 (`grok-imagine-image`, `grok-imagine-image-quality`) and
    64000 (`grok-imagine-image-2.0`), and the three video rows held none. `capabilities`, with a
    `reasoning_effort` list and a `default_reasoning_effort`, was on `grok-4.3`, `grok-4.5`, `grok-4.6` and
    `grok-4.7` and on no other row.
  - `POST /chat/completions`, not streamed, with a small `max_tokens`, answered a 200 for the seven models of
    [XAI-14](#XAI-14), with `choices[0].message` and a `usage` that held `reasoning_tokens` under
    `completion_tokens_details`. The `model` of the reply equalled the one asked for. It answered a 400 for
    `grok-4.20-multi-agent-0309` and for each of the six `grok-imagine-` ids. Streaming and tool calls were not
    tried.
  - `POST /responses` answered a 200 for `grok-build-0.1` and was tried for no other model.
- A model the account does not have was answered with a 404 and a body whose `error` held the account's team
  identifier. A bearer that was not a token was answered with a 400, not a 401: a body with `code`
  `invalid-argument` and an `error` that said the key was incorrect, and no `www-authenticate` header.
- A bearer that was a JWT altered after it was issued was answered with a 403, not a 401, by `GET /models`: the
  last eight characters replaced, a character of the signature changed, a character of the payload changed and
  the signature emptied each gave a 403 with `code` `unauthenticated:bad-credentials` and an `error` that said the
  OAuth2 access token could not be validated. A bearer that was not a JWT (`not-a-token`, `aaaa.bbbb.cccc`) gave
  the 400 above. This was tried on 2026-10-26 with one account and on `GET /models` alone. A token that had
  expired at the service, or whose revocation had taken effect, was not tried, and no refusal for a plan was seen.

## Not observed

Bravebot has not observed these, and the clauses state no value for them. Where another party has reported
something about one, the report is named under it. A report is stated but not confirmed: it comes from a program
other than bravebot and from an account bravebot does not hold, and no clause depends on it for a value.

- how long a refresh token lasts (tracked by brave/bravebot#3377). No source found states a lifetime. xAI's
  documentation pages for the X subscription and its discovery document state none. xAI's open-source client
  `xai-org/grok-build` says in `crates/codegen/xai-grok-login/src/error.rs` that `invalid_grant` means the refresh
  token is expired, reused or revoked and that the issuer does not distinguish these, so a refusal alone does not
  show a lifetime;
- whether the previous refresh token still works after a refresh replaced it, which was not tried because a
  replay may end the sign-in. Reported, not confirmed: the refresh token is single use, and the previous one is
  refused with `invalid_grant` ("Refresh token has been revoked") after another process refreshed (opencode issue
  37059, Hermes Agent issues 43589 and 95431). `xai-org/grok-build` comments that a refresh token can be used only
  once (`xai-grok-login/src/storage.rs`) and that re-sending a rotated token trips the issuer's reuse detection and
  revokes its successor (`xai-grok-login/src/oidc/refresh.rs`). That client's `ROTATION_GRACE_MS` of 60 seconds is
  its own conservative bound, and no source states whether the server accepts the previous token for a grace
  period;
- the codes `slow_down`, `access_denied` and `authorization_denied`;
- how long an access token issued before a revocation keeps being accepted;
- what the serving host answers an access token that has passed its expiry, and a 401 to any token (an altered
  token was answered with a 403), and what `POST /chat/completions` answers a token it cannot validate, since that
  was tried on `GET /models` alone;
- what the service answers an account on a plan that cannot use this, since the one signed in was accepted.
  [XAI-18](#XAI-18) tells a 403 that outlives a renewal as a refusal for the account, and that sentence rests on
  this being unobserved (tracked by brave/bravebot#3377). Reported, not confirmed, by users of other programs
  that sign in the same way: a 403 for `GET /models` and `POST /chat/completions` on a standard SuperGrok account,
  with a body saying the caller lacks permission or has no active Grok subscription (Hermes Agent issue 26847,
  opened 2026-05-16, whose later comments dispute that the plan is the cause and name a monthly quota); a 403 with
  the code `personal-team-blocked:spending-limit` (Hermes Agent issue 70203); a 402 with that code on an active
  SuperGrok session (broccoli-core issue 23, opened 2026-09-01); and a 403 with the code
  `unauthenticated:bad-credentials` for a stale access token, where a refresh is the remedy (Hermes Agent issue
  108122). Hermes Agent's xAI guide says the service "has been seen to restrict OAuth API access to specific
  SuperGrok tiers" without a status, and xAI's page `x.ai/news/grok-hermes` says that connecting Grok to Hermes is
  "available on every tier". No source states what the service answers a free or X Basic account, or the status of
  a refresh for an account whose plan is refused;
- streaming and tool calls on the serving host, and a request that carries an effort level;
- `https://cli-chat-proxy.grok.com`, the host xAI's own command line client uses with headers of its own, which
  opencode does not use and this program does not ask;
- whether the service answers a request that names an alias, such as `grok-code-fast-1`, under the canonical id
  or under the alias;
- what any other plan, account or day would show.

## What this changes elsewhere

- **Credential protection.** [CRED-14](credential-protection.md#CRED-14) and [CRED-25](credential-protection.md#CRED-25)
  cover this sign-in's file and its account in `doctor`, and [CRED-26](credential-protection.md#CRED-26) its
  binding.
- **The command line.** [CLI-18](cli.md#CLI-18) lists six ways and states `logout xai`. [CLI-23](cli.md#CLI-23)
  takes `xai` as a way.
- **Incognito.** [INCOG-7](incognito.md#INCOG-7) refuses the sign-in and allows signing out, and
  [INCOG-8](incognito.md#INCOG-8) lists the sign-in's file among those an incognito session still writes: a
  stored sign-in is renewed and a sign-in xAI has ended is removed, as the ChatGPT account's is.
- **Layering.** [LAYER-1](layering.md#LAYER-1)'s row for `bravebot-skus` says the crate also runs this sign-in
  and holds the file and renewal both sign-ins share.
- **Network egress.** [network-egress.md](network-egress.md) records that `bravebot-skus` builds an HTTP client
  for each sign-in, that the xAI sign-in follows no redirect, and that its destinations are
  [XAI-1](#XAI-1)'s.
- **The ChatGPT sign-in.** [chatgpt-sign-in.md](chatgpt-sign-in.md) governs the shared file and renewer as
  well, and its pins of who names a host read them. The re-read before a removal ([XAI-6](#XAI-6)) is in the
  shared renewer and so holds for the ChatGPT sign-in as well, and so does keeping no connection between the
  requests of the shared agent ([XAI-3](#XAI-3)).
- **The desktop failure card.** The title of the card for a failure of an account names neither service, since
  one card serves both accounts and the window keeps no table of service names. The sentence under it names the
  service. `ui/src/renderer/failure.ts` and `ui/docs/phase-0-rpc-protocol.md` say so.
- **The Agent Client Protocol.** [ACP-8](acp.md#ACP-8) says an editor is told an account's failure in its
  sentence, for either account.

## Known costs

- The refresh token is a bearer secret in a file for as long as the sign-in lasts. Anything running as the
  person can read it, as it can read the ChatGPT sign-in and the gateway keys.
- A code of another shape than the one the service gave, that holds a space, is refused as unreadable and shown to
  nobody, and the sign-in ends ([XAI-3](#XAI-3)).
- The address and the code are shown to the person as the reply gave them, after a check that they are
  printable and that the address is `https`. A reply from the authorization server that gave an address of
  another host would be shown, and the person would open it. The destinations are constants and the reply
  arrives over a connection to one of them, so the reply is xAI's.
- A sign-in ends at xAI and this program learns of it at the next refresh, which is the first request after the
  access token is within a minute of its expiry, and so up to the access token's lifetime after it ended, which
  was 21600 seconds on the one observation.
- A process killed, or stopped by Ctrl-C, after the service has replaced the refresh token and before the file is
  written loses the sign-in, since the refresh token in the file is spent. The next process that finds the claim
  of the one that died waits for it silently until it is sixty seconds old, and then ends the sign-in ([XAI-6](#XAI-6)).
- The file is written beside and renamed over without a sync, so a power loss after the service replaced the
  refresh token can leave an empty file, and a sign-in that was spent.
- A one-shot or plain run that succeeds prints no notice that a renewal could not be saved, and `--json` shows
  none: its progress keeps notices for a run that ends in failure, as for the ChatGPT account, and the next start
  finds the file's refresh token spent ([XAI-17](#XAI-17)).
- The command and agent glue for this account mirrors the ChatGPT account's (`Standing`, `standing_of`, `Facts`,
  `StoredSignIn`, `Accounts`). Sharing it is left until there is a third account.
- The `exp` claim of an access token is read without checking the token, so a token that states another time
  than its own is renewed at that time, held to between five minutes and a day from the reply ([XAI-3](#XAI-3)).
- The name an xAI reply reports is compared for no `xai/` name, the canonical ids included. A turn on
  `xai/grok-4.7` that the service answers with `grok-4.3` is not reported as a model that was not served, although
  the name in the reply equalled the one asked for on each of the seven canonical ids observed. What name a
  reply reports for a request that names an alias was not observed, and the exemption is for every `xai/` name
  and not for the aliases alone ([XAI-16](#XAI-16)).
- A claim is judged abandoned at sixty seconds whatever its holder is doing, so a holder that was stopped for
  longer than a request may take, such as a machine that slept, can have its claim broken. The process that broke
  it presents a refresh token the holder has spent and is refused for it. It reads the file again before it
  removes anything and uses what the holder wrote, and where the holder wrote nothing before it was stopped
  for good the sign-in ends ([XAI-6](#XAI-6)).
- One account is signed in at a time on one machine.
- No effort level is sent to the account, so a level chosen in a session does nothing for a model of it, and the
  four models whose listing states levels are asked at the service's default ([XAI-16](#XAI-16)).
- Streaming, tool calls and a prefix marked for caching were not tried against the service. A turn on the
  account sends them as it sends them to any gateway, and a request the service refuses for the mark is sent
  again without it, once for the process, at the cost of one more request ([BACKEND-22](backends.md#BACKEND-22)).
- A request the service refuses with a 400, a 401 or a 403 spends a refresh and costs at least one more request
  before it reaches the turn, the refused token is presented twice before it is replaced where the status is a
  400, and the refresh token is replaced each time ([XAI-18](#XAI-18)). A rate-limited authorization server replaces the status the request failed
  with in what a person is told, so a prompt that is too long is not shortened.
- A request the service answers with a 403 renews the token once and is sent again, as a 400 or a 401 is, and
  only a 403 after that is told as a refusal for the account ([XAI-18](#XAI-18)). A refusal for a plan, if the
  service makes one, costs a refresh and one more request before it is told, and was not observed by bravebot
  (users of other programs state it, which is not confirmed, [Not observed](#not-observed)). The rule reads
  the status alone, so a 403 for another reason is told the same way and a person is sent to check a plan that may
  not be at fault. A 403 on a token refresh is told as a refresh that failed. A listing refused with a 403 is
  asked again once with a new token like a turn's request, and refused again is said as a listing that could not be
  read, in the sentence that suggests signing in again, which does not change a refusal for the account. What
  xAI answers an account on a plan that cannot use this is tracked by brave/bravebot#3377.
- A start that asks which model to use sends the stored token to the serving host for the listing before the
  person has chosen anything ([BACKEND-39](backends.md#BACKEND-39)). It does so only in a terminal outside an
  incognito session, with the sign-in stored, nothing else configured and no model named, so it is the one place
  the token goes for a person who has not yet named a model. A person with both accounts signed in has both
  listings asked, one after the other, so the question waits for the two answers, and an account that is down
  is said above the other's rows.
- A person with an API key entry for xAI and a sign-in cannot use both under one name, and what a draft change
  to the table of known names would do to that is in [XAI-13](#XAI-13).
- The rule of [XAI-14](#XAI-14) is on the id. A model whose id begins another way, as a family xAI adds later
  might, is not offered until a release changes the rule, and is reached by naming it `xai/<id>`. One whose id
  passes and that the chat route refuses is offered, and its first request is refused with the status the
  service gave. The aliases a row lists, such as `grok-code-fast-1` for `grok-build-0.1`, are not offered, and a
  request that names one is sent as named, with the reply's name not compared ([XAI-16](#XAI-16)).
