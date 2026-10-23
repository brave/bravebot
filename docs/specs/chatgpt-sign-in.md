---
id: CHATGPT
title: Signing in with a ChatGPT account
status: proposed
governs:
  - crates/cli/src/auth.rs
  - crates/config/src/provider.rs
  - crates/agent/src/backend.rs
  - crates/aichat/src/lib.rs
  - crates/cli/src/chatgpt.rs
  - crates/skus/src/chatgpt/mod.rs
  - crates/skus/src/chatgpt/account.rs
  - crates/skus/src/chatgpt/authorize.rs
  - crates/skus/src/chatgpt/encoding.rs
  - crates/skus/src/chatgpt/exchange.rs
  - crates/skus/src/chatgpt/listener.rs
  - crates/skus/src/chatgpt/renew.rs
documented-by:
  - none (gap: a page on signing in with a ChatGPT account, owed until the design is built)
---

## Scope

A person with a ChatGPT Plus or Pro subscription signs in to it in a browser, and the models the
account may use are then offered in the picker beside every other roster. The sign-in is the OAuth 2.0
authorization code flow with PKCE against OpenAI's authorization server, with a listener on the loopback
address receiving the redirect. opencode's "Sign in with ChatGPT" signs in this way. OpenAI issues a client
to each program that registers one at its first sign-in, so this program presents no other program's client
identifier.

Reaching OpenAI with a pay-per-token API key is a different route, the `openai` entry that
[backends.md](backends.md) describes. It needs none of this and this needs none of it.

The command is built: `auth login chatgpt` signs in and stores the tokens, `auth logout chatgpt` forgets
them, and `auth status` and the list of ways report them. [What is built](#what-is-built) says which parts of
which clauses that is. [What the exchange has to record](#what-the-exchange-has-to-record) lists what a sign-in made with a real account has observed and what it has not. A clause reads
`verified-by: none` until the work it describes lands, and a clause built in part says which part is not. The
points where the unbuilt work changes a built spec are named in
[What this changes elsewhere](#what-this-changes-elsewhere), and those specs are edited when it lands.

The hosts, parameters, field names and headers the clauses state are read from opencode's source
(`packages/core/src/plugin/provider/chatgpt.ts`, at commit `ac68d6a929`), which is another program's code.
Each is a value to confirm with a sign-in unless the list below marks it observed, and [What the exchange has to record](#what-the-exchange-has-to-record)
lists them beside what no source states. Whether OpenAI's terms allow a program to use a subscription this
way, and whether OpenAI registers a client for this program, are
[decisions left open](#decisions-left-open). The command is built on the values as stated, and a release
that carries it waits on the first of those decisions. Sending a request with the sign-in waits on the
Responses protocol of #2994.

The flow carries no workspace content and no model output. The rule that untrusted content never
reaches the driver or the planner is [AGENTS.md](../../agents/AGENTS.md), and
[CHATGPT-5](#CHATGPT-5) checks this flow against it. Where each part lives is
[layering.md](layering.md). The other sign-in that stores a person's subscription is
[premium-credentials.md](premium-credentials.md), what a credential owes is
[credential-protection.md](credential-protection.md), and the command this extends is
`auth login` in [cli.md](cli.md).

The flow:

1. A listener starts on `127.0.0.1` at a port the operating system chooses, and the browser is opened at
   the authorization endpoint on `auth.openai.com`. The request asks for `response_type=code`, the scopes
   and the `resource` of [CHATGPT-3](#CHATGPT-3), a `code_challenge` with method `S256`, a random `state`
   and `nonce`, and redirects to the listener's `/auth/callback`. At a first sign-in it names a placeholder
   client and the program (`agent_name_hint`), and a host identifier generated here. OpenAI registers a
   client for that and returns its `client_id` on the callback.
2. The person signs in at OpenAI, in the browser, and approves the request, which includes allowing the
   program to use the subscription's tokens.
3. The browser is sent back to the listener with a `code`, the `state` and, at a first sign-in, the
   `client_id`. This program exchanges the code and its PKCE verifier, under that client, for an access
   token, an ID token and a refresh token, and checks the reply granted the scope that allows this use.
4. Requests to the model service carry the access token. It is replaced, using the refresh token, before
   it expires, during a turn.

## Clauses

<a id="CHATGPT-1"></a>
### CHATGPT-1: only a command a person types signs in

`bravebot auth login chatgpt` is the way in, run by a person at a terminal. It is a fifth way on the list
`auth login` prints, and `bravebot auth logout chatgpt` and `bravebot auth status chatgpt` are its partners
([CHATGPT-11](#CHATGPT-11)). No tool the planner calls, no hook, no settings file, no environment variable
and no slash command in a session starts the flow, and a line that arrives through a quarantined source and
reads like the command is not the command.

A turn whose model is named `chatgpt/<name>` on the command line or by a settings file, where no gateway
entry has that id ([CHATGPT-9](#CHATGPT-9)), and that finds no usable sign-in, is refused before a request
is sent. The refusal is a sentence from [CHATGPT-8](#CHATGPT-8), and it names the command.

The way is named `chatgpt`. `openai` is the id of the gateway reached with `OPENAI_API_KEY`, and
`auth status openai` would name two credentials.

**Not built:** the refusal of a turn whose model is named `chatgpt/<name>`, which comes with the account's
models ([CHATGPT-9](#CHATGPT-9)).

**Why.** The flow opens a browser, holds a port and ends with a credential on disk. Each is a decision
about a person's machine and account, and a person's own typing is the one source for it that no content
can author. A session signs in to AWS when it opens, because that sign-in is the AWS CLI's, run in the
terminal the session already holds. This one has to hold a port for minutes and show a URL a person
acts on, and a person on another machine has to be told which port to forward. A session in an editor, a
desktop window or a background run has nowhere to show the URL. A session with no service configured
already takes this road: it says the command to type and does not start the work.

`verified-by: bravebot_cli::layering::the_chatgpt_sign_in_is_started_by_the_command_and_nothing_else`
`verified-by: bravebot_cli::running::auth_login_chatgpt_is_refused_before_a_browser_opens_where_it_cannot_be_done`
`verified-by: bravebot_cli::auth::every_way_is_listed_and_only_a_held_one_says_what_it_holds`
`verified-by: bravebot_cli::auth::a_way_is_picked_by_its_number_or_its_name`

<a id="CHATGPT-2"></a>
### CHATGPT-2: a token reaches only the hosts compiled in for it

Four destinations are used, each a constant in the source:

- the authorization page, `https://auth.openai.com/api/accounts/authorize`, which the browser is opened at;
- the token endpoint, `https://auth.openai.com/api/accounts/oauth/token`, which takes the code exchange and
  the refresh;
- the key set, `https://auth.openai.com/.well-known/jwks.json`, which the ID token is verified against
  ([CHATGPT-5](#CHATGPT-5));
- the serving host, `https://api.openai.com/v1`, which takes the model requests and the listing of
  [CHATGPT-9](#CHATGPT-9).

A page, a reply, a model, a settings file at any layer, an environment variable and a redirect name none of
them, and none of them is looked up from anything fetched, as the backends spec holds for a gateway's known
name. A test points the flow at a server of its own through a seam that exists only in test builds.

The access token is sent to the serving host and to no other. The refresh token and the code are sent to
the token endpoint and to no other. The request for the key set carries no credential. A request to any of
the four follows no redirect, whichever client makes it, the one in the crate that signs in or the one that
sends a model request: a redirect in answer to it is a failure. A gateway key, including the `openai` gateway's, is never sent to these hosts,
and the tokens of this account are never sent to a gateway.

A model of this service is answered with the stored sign-in and with nothing else. `OPENAI_API_KEY`, a
gateway key and any other variable are not looked for.

**Not built:** the key set and the serving host, and the answering of a model of this service with the stored
sign-in. The flow sends to the authorization page and the token endpoint only.

**Why.** A settings file may name a destination, which is right where the credential is one the person
typed for that destination. A checkout's settings file naming where an account's refresh token goes would
send a person's subscription to whoever wrote the file, and a token whose host came from a reply would go
wherever the reply said. Looking for no other credential keeps the two OpenAI routes from being mistaken
for each other: a person who has a key and a subscription gets the one their model name selects
([CHATGPT-9](#CHATGPT-9)). A redirect re-sends the request, and with it the token, to a host nobody
compiled in.

`verified-by: bravebot_skus::chatgpt::authorize::a_first_sign_in_asks_for_a_client_and_carries_the_parameters_listed_and_no_other`
`verified-by: bravebot_skus::chatgpt::a_redirect_in_answer_to_the_exchange_is_not_followed`
`verified-by: bravebot_skus::chatgpt::a_first_sign_in_exchanges_the_code_under_the_issued_client_and_stores_the_account`

<a id="CHATGPT-3"></a>
### CHATGPT-3: the values the request depends on are generated here, for one sign-in

The authorization request carries these parameters, and no other:

- `response_type=code`;
- `scope=openid profile email offline_access resource.invoke chatgpt.tokens.use.direct`, and
  `resource=https://api.openai.com/v1`, which every token request repeats;
- a `code_challenge` of method `S256`, never `plain`;
- a `state` and a `nonce`;
- the redirect URI of [CHATGPT-4](#CHATGPT-4);
- the client, which is the one OpenAI issued where the sign-in file holds one. Otherwise it is the
  placeholder `dynamic_agent_client` with `agent_name_hint=bravebot`, which asks OpenAI to register a client
  for this program;
- `ext_agent_host_id`, a `urn:uuid:` value generated here at a sign-in that has none stored and kept in the
  sign-in file, so a sign-in over a stored one names the same host.

The PKCE verifier, the `state` and the `nonce` are each 32 bytes from the operating system's random source,
encoded without padding, and used for one sign-in: a second `auth login chatgpt` makes new ones, and a
callback carrying an earlier flow's `state` is not the callback. The verifier leaves the process once, in
the code exchange, and is cleared afterwards, as every credential buffer is. The request sends no
`originator` and names no other program: the program names itself, in `agent_name_hint`.

The URL the browser is opened at is built from those values and from constants. Nothing in it comes from a
settings file, a model or a page.

**Why.** A verifier or a state a reader could guess, or one reused, lets another program on the machine
complete or replay a sign-in. The URL is passed to the program that opens the browser as an argument, which
other accounts on the machine can read while it runs. It holds nothing that can complete a sign-in: the
challenge is public by construction, and a `code` obtained with another program's challenge does not
exchange against this verifier. It does hold the `state`, which a reader of the argument list could send to
the listener to end a sign-in ([Known costs](#known-costs)). A client registered for this program, named
by it, says truthfully which program is asking, where another program's client identifier would say it is
that program.

`verified-by: bravebot_skus::chatgpt::authorize::a_first_sign_in_asks_for_a_client_and_carries_the_parameters_listed_and_no_other`
`verified-by: bravebot_skus::chatgpt::authorize::a_sign_in_over_a_stored_one_presents_its_client_and_host_and_asks_for_no_client`
`verified-by: bravebot_skus::chatgpt::authorize::the_challenge_is_the_hash_of_the_verifier_and_the_verifier_is_not_in_the_url`
`verified-by: bravebot_skus::chatgpt::authorize::each_sign_in_generates_its_own_verifier_state_and_nonce`
`verified-by: bravebot_skus::chatgpt::authorize::the_values_are_percent_encoded_in_the_url`
`verified-by: bravebot_skus::chatgpt::encoding::a_challenge_is_the_rfc_7636_one_for_its_verifier`
`verified-by: bravebot_skus::chatgpt::encoding::a_random_value_is_43_url_safe_characters_and_a_new_one_each_time`
`verified-by: bravebot_skus::chatgpt::a_first_sign_in_exchanges_the_code_under_the_issued_client_and_stores_the_account`

<a id="CHATGPT-4"></a>
### CHATGPT-4: the listener takes the one callback that carries its state

The listener binds `127.0.0.1`, and nothing else, on a port the operating system chooses, and the redirect
URI is `http://127.0.0.1:<port>/auth/callback` with that port. The authorization server accepts a redirect
to any such port for a client registered this way. Where the listener cannot start, the sign-in says so,
opens no browser and stops. The port is stated to the person once it is bound ([CHATGPT-13](#CHATGPT-13)).

A request is the callback only if it is a `GET` for `/auth/callback` whose `state` equals the one
generated, compared in constant time. The first callback is the only one served, and the listener closes
after it. It closes without one after five minutes, and says so. Any other request, whatever its method,
path or `state`, is answered with a fixed status and a fixed page and is ignored, and the listener goes on
waiting. A connection is read for at most five seconds and at most 8 KiB, and one that has not sent a
complete request line by then is closed. Connections are served at the same time, so one that sends nothing delays no other. From the query
it reads `code`, `state` and `client_id` and nothing else, and it reads no header. A callback with no
`code` ends the sign-in, whatever else it carries, including an `error` the server put there. The
`client_id` is used only where no client was stored, and only if it is a non-empty string of at most 128
characters holding letters, digits, hyphens, underscores and periods. A callback with no such `client_id`
at a first sign-in ends it, and so does one whose `client_id` differs from a stored one. The reply to a
callback is one fixed page that loads nothing and repeats nothing from the request. After any reply the
connection is told nothing more will be sent and what it still sends is discarded unread, up to 64 KiB and half a
second, so that closing it does not reset it and take the page with it.

The query string and the values read from it are held in memory for the exchange and are written nowhere:
not to a log, a trace, an error message or the terminal ([CHATGPT-8](#CHATGPT-8)).

**Why.** A port on the loopback address is reachable by every program on the machine, and by a page in the
person's browser that makes a request to it. The `state` is what tells the person's own redirect from
those. A request that fails it is not the callback, and counting it as one would let a single request from
a page or a program end every sign-in. A listener on all interfaces would take a callback from the
network. A redirect to the literal address `127.0.0.1` is not resolved by name, so the browser reaches the
one address the listener is bound to. A page that repeated the query would put a `code` in a second place,
and a log of it would keep a `code` and a `state` for as long as the log. Reading only the three named
parameters keeps a callback, which is bytes a browser sent, down to a `code` to hand on, a `state` to
compare and a client identifier that is checked for its shape and then verified by the exchange, and by the ID
token once that is verified ([CHATGPT-5](#CHATGPT-5)).

`verified-by: bravebot_skus::chatgpt::listener::the_listener_is_bound_to_the_loopback_address_only`
`verified-by: bravebot_skus::chatgpt::listener::the_callback_with_the_state_is_served_and_answered_with_a_page_that_repeats_nothing`
`verified-by: bravebot_skus::chatgpt::listener::a_request_that_is_not_the_callback_is_answered_and_ignored_and_the_wait_goes_on`
`verified-by: bravebot_skus::chatgpt::listener::a_callback_with_no_code_is_returned_as_one_with_none`
`verified-by: bravebot_skus::chatgpt::listener::only_the_first_callback_is_served`
`verified-by: bravebot_skus::chatgpt::listener::the_query_is_decoded_and_the_first_of_a_repeated_name_is_read`
`verified-by: bravebot_skus::chatgpt::listener::a_client_identifier_of_the_wrong_shape_is_dropped`
`verified-by: bravebot_skus::chatgpt::listener::a_connection_that_sends_nothing_delays_no_other`
`verified-by: bravebot_skus::chatgpt::listener::a_connection_that_sends_nothing_is_closed_when_its_time_is_up`
`verified-by: bravebot_skus::chatgpt::listener::a_request_line_longer_than_the_limit_is_closed_without_an_answer`
`verified-by: bravebot_skus::chatgpt::listener::the_request_line_is_all_that_is_read`
`verified-by: bravebot_skus::chatgpt::listener::each_page_carries_the_callers_words_escaped_and_the_two_differ`
`verified-by: bravebot_skus::chatgpt::listener::a_reply_reaches_a_client_that_sent_more_than_its_request_line`
`verified-by: bravebot_skus::chatgpt::listener::a_wait_that_sees_no_callback_ends_at_the_bound`
`verified-by: bravebot_skus::chatgpt::encoding::two_values_are_the_same_only_when_every_byte_is`
`verified-by: bravebot_skus::chatgpt::the_browser_has_five_minutes_to_return_by_default`
`verified-by: bravebot_skus::chatgpt::a_callback_naming_another_client_than_the_stored_one_ends_the_sign_in_unexchanged`
`verified-by: bravebot_skus::chatgpt::a_callback_without_a_code_or_without_a_usable_first_client_stops_before_the_exchange`

<a id="CHATGPT-5"></a>
### CHATGPT-5: nothing the flow reads from outside reaches the driver or the planner

The sign-in runs as a command before any session, so no planner exists whose context it could enter. The
refresh of [CHATGPT-7](#CHATGPT-7) runs inside a turn, in the same crate, and its outcome reaches the agent.
Both read bytes this program did not write: a callback from a browser, and replies from the authorization
server and its key set. The crate compares and parses them and hands none of them to a turn, a trace or a
message to the planner. That is the argument [premium-credentials.md](premium-credentials.md) makes for its own flow. It
is checked here against [reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md), shape by
shape:

- **A branch on untrusted bytes.** The comparisons are the `state` against the value generated here, and a
  reply's status and the fields the next step needs. They decide this program's own sign-in. What crosses
  into the driver is a token and a category, which is the class of a reply's status, as the failure
  categories of the backends spec are for the reply of any other service. A refused refresh adds one value,
  that the refresh token is no longer valid, chosen by comparing one field of the error body with `invalid_grant`
  ([CHATGPT-7](#CHATGPT-7)). A refused model request adds one more, that the account may not be used this way,
  chosen by comparing the error code of the body with a closed list of codes the exchange records
  ([CHATGPT-8](#CHATGPT-8)); that comparison is made in the client that sends the request, which is not the
  driver. A turn already branches on the categories, and none is content anyone authored. The
  rest of a reply's body, its error text and its claims stay in the crate. That is the ordinary case the
  document's section on the inverse mistake describes, and the same footing as the fields of a model
  listing the code already reads.
- **The same branch moved into the kernel.** None is moved. The flow's code is in a crate that depends on
  neither `bravebot-core` nor `bravebot-agent`, which [layering.md](layering.md) treats as one driver, so no
  decision about a reply's content is taken in the driver.
- **A `declassify` outside the gates.** The flow has none, since it holds no labelled value.
- **A `Labelled` built by hand.** The flow builds none.

Three conditions keep the argument true, and a change that breaks one is a change to this clause:

1. The crate holds no `Labelled` and carries no workspace content and no model output, as `bravebot-skus`
   does.
2. What the agent receives from it is a token, in the secret type that
   [credential-protection.md](credential-protection.md) describes, and a value from the closed set of
   [CHATGPT-8](#CHATGPT-8). No string taken from a reply is part of either.
3. The ID token is verified once, at the sign-in, and no claim of it is stored, sent or displayed. This
   is not built: the reply's `id_token` is dropped unread, since the verification needs a signature
   verifier this crate does not hold ([Decisions left open](#decisions-left-open)). Its
   signature is checked against the key set with RS256 only, and its issuer, audience (the client of the
   sign-in), expiry, nonce (the one sent) and subject (present and not empty) are compared. No other claim
   is read. The access token's expiry is read from the `expires_in` field of the reply, not from the token,
   and is compared with the clock. Claims such as an organisation's name are text somebody else chose.

No exception is added to [labels.md](labels.md), and no count it pins moves.

**Why.** The rule protects the planner and the driver from content an attacker could write. A callback is
the one place here where bytes from outside arrive, and whoever wrote them could reach a loopback port: any
program on the machine, or a page in the browser. Nothing they could write reaches a turn, because the
only things read from the callback are a value compared with one generated here, and a `code` and a client
identifier handed to the authorization server, which accepts the pair only if it issued the `code` to that
client for this verifier. A reply from that server is the other place, and what leaves the crate from it is a
token that is never read, or a category computed from a status.

`verified-by: none`

<a id="CHATGPT-6"></a>
### CHATGPT-6: the tokens are one file the account owns

The sign-in is kept in `chatgpt-account.json` in the state directory. It holds a version, the access token, the
refresh token, the instant the access token expires, the client OpenAI issued ([CHATGPT-3](#CHATGPT-3)) and
the host identifier, and not the ID token. The file is created before anything is written to it, reachable by the account that signed in
and by no other, with the protection the Leo credentials file has: mode 0600 on Unix and an access-control
list for one account on Windows, with nothing inherited. A platform with neither has the sign-in refused. It
is written beside the file and renamed over it, so a write that stops part way leaves the last good file.
No settings layer, `provider` block or environment variable holds a token or names one, and the file is not
`settings.json`.

A file that cannot be read, is empty, or holds the fields of another version is reported with the remedy
and is not read as an absent sign-in, as a stored Leo batch that cannot be spent is.

Each token is a credential at a tier, in the terms of [credential-protection.md](credential-protection.md):

- **Both are Held.** The refresh token mints access tokens without asking anyone, so a value this program
  renews without further authority is permanent whatever its stated lifetime.
- **What would end each is recorded:** signing out of the ChatGPT account, or removing this program's
  access in the account's settings, at OpenAI. This program cannot do either for the person.
  `bravebot auth logout chatgpt` removes the copy here and ends nothing at OpenAI. Whether an access token
  minted before a revocation survives it is a fact the exchange records, and none has: the record says an
  access token already issued may run to its own expiry, and `doctor` says so with the words that this has
  not been observed, not that nothing survives.
- **Neither is bound to its presenter.** Nothing asks the authorization server for a sender-constrained
  form, so the record says bearer, with the answer `NotAttempted`, until the exchange records whether one
  is offered.

A token is held in the secret type that spec describes, so it is redacted wherever it is printed, and its
buffers are cleared and kept out of crash artifacts. It reaches no program a turn starts, no prompt and no
record, and it is never passed as a command-line argument.

The record holds one entry for the pair, `Held::ChatgptAccount`, since the two tokens are one arrangement ended at
one surface. Its walk is the three drops an AWS session has, on the conditions that the token is presented by this
program, that nothing here asks OpenAI to narrow it to a run, and that the refresh token replaces it without
asking anybody, each one nobody attempted because no narrower or bound form has been asked for. It is a bearer
secret with the answer `NotAttempted`, and a leak of it has no figure for how quickly it would be noticed.
[CHATGPT-11](#CHATGPT-11) is where `doctor` reports it.

**Why.** A file of its own because the shape differs from the gateway keys, which are one string under an id
with no expiry, and from the Leo batch, which is spent a credential at a time. Held because the arrangement
is the one that spec names as the false pass, a short-lived token that the holder renews by itself. A record
of what ends the credential is what a person who suspects a leak needs, and it can only be written down
while the arrangement is understood.

`verified-by: bravebot_skus::chatgpt::account::a_stored_sign_in_is_read_back_field_for_field`
`verified-by: bravebot_skus::chatgpt::account::the_file_holds_the_named_fields_and_nothing_else`
`verified-by: bravebot_skus::chatgpt::account::no_file_is_no_sign_in`
`verified-by: bravebot_skus::chatgpt::account::a_file_that_cannot_be_used_is_an_error_naming_its_defect_and_not_an_absent_sign_in`
`verified-by: bravebot_skus::chatgpt::account::a_refusal_of_a_malformed_file_quotes_none_of_it`
`verified-by: bravebot_config::lib::a_chatgpt_sign_in_is_held_renewed_unaided_and_possibly_outlives_revocation`
`verified-by: bravebot_skus::chatgpt::account::the_tokens_are_redacted_wherever_the_account_is_printed`
`verified-by: bravebot_skus::chatgpt::account::the_file_is_readable_by_its_owner_alone_and_so_is_its_replacement`
`verified-by: bravebot_skus::chatgpt::account::a_write_that_stops_part_way_leaves_the_last_good_file_and_no_temporary`
`verified-by: bravebot_skus::chatgpt::account::a_second_sign_in_replaces_the_first_and_forgetting_removes_it`
`verified-by: bravebot_skus::chatgpt::account::a_write_waits_for_the_claim_on_this_file_and_not_for_the_leo_one`
`verified-by: bravebot_skus::chatgpt::account::a_claim_on_this_file_is_waited_on_until_it_is_a_minute_old`
`verified-by: bravebot_skus::chatgpt::account::the_temporary_of_a_name_that_is_not_utf8_is_named_from_its_bytes`
`verified-by: bravebot_agent::home::a_directory_named_in_the_environment_is_the_state_directory_in_every_crate`
`verified-by: bravebot_agent::home::a_value_that_names_no_directory_leaves_no_state_directory_to_guess`
`verified-by: bravebot_agent::confine::a_stage_reads_the_machine_and_is_refused_the_credential_locations`
`verified-by: bravebot_sandbox::base::the_run_base_reads_the_session_records_and_no_other_file_in_the_state_directory`

<a id="CHATGPT-7"></a>
### CHATGPT-7: an access token is replaced before it expires

A request to the serving host is built from an access token whose expiry is more than a minute away. The
expiry is the instant of the reply plus its `expires_in`, or plus 3600 seconds where the reply has none.
Otherwise the refresh token is presented to the token endpoint, with the stored client and the `resource`
of [CHATGPT-3](#CHATGPT-3), the reply is written to the file, and only then is the new access token used. A
refresh reply that grants no refresh token keeps the one presented. No request is sent with a token past its
expiry. A token the serving host refuses before its expiry,
whether the clock here runs behind or the token was revoked early, is replaced in the same way once, and the
request is sent once more, unless the refusal carries a code of [CHATGPT-8](#CHATGPT-8)'s closed list: those
say the account may not be used this way, and a new token does not change that. A second refusal ends the
turn with a sentence from [CHATGPT-8](#CHATGPT-8). Where the refresh fails other
than by ending the sign-in and the access token in hand has not yet passed its expiry, the request is sent
with it.

The refresh is taken under an exclusive claim on the file, as the Leo store takes one for a spend, and the
file is read again once the claim is held. Every write of the file and its removal is made under the claim.
A process that finds a token other than the one it was about to replace uses it and presents nothing. The
refresh request has a deadline of thirty seconds, and a claim is judged abandoned only after sixty, so a
refresh still waiting for its reply is never broken. A claim older than that belongs to a process that died
holding it and is broken as the Leo store breaks one, by moving it aside and judging it there.

A refresh the authorization server refuses as the refresh token no longer being valid ends the sign-in. That
is a status of 400 or 401 whose error code is `invalid_grant`, which is RFC 6749 section 5.2's code for it and
is not observed from this server: a refusal with any other code or status is a failure of the request and ends
nothing. The file is removed, and the turn says the sign-in has ended, whether or not the file could be
removed. The file was read with the claim held, so it holds the refresh token that was refused or one this
process replaced and so killed. Any other failure of the refresh, a connection that did not complete or
a reply that says the service is busy or unavailable, leaves the file as it is and says which of the
backends spec's failure categories it was. A write of the reply that fails is reported, the new tokens are
used from memory for the rest of the process, and the process tries the write again before each later request, under the claim,
and only if the file still holds the token this process replaced. Where it holds another, or none, the tokens in
memory are dropped.

**Why.** An authorization server may replace the refresh token each time it is used, and then the copy a
second process read before the first's reply is dead. A session, a background run and a one-shot run on one
machine present it together as a matter of course, and the one that loses ends the sign-in for all of them.
The claim is held across a network round trip, so it has an age longer than that trip's deadline: the Leo
claim is judged abandoned after seconds because its hold is one local read and write. The reply is written
first because a process that stops between receiving it and writing it has spent a refresh token it cannot
present again. This server replaces the refresh token on use, which a refresh against it has shown, so
the claim and the early write are required here. A server that did not replace it would lose nothing to them. Removing the file
on an ended sign-in leaves `auth status` and the picker saying what is true, and stops every later turn
from presenting a token that cannot work.

**Not built:** the caller. Nothing sends a request to the serving host yet, so the refusal of a token before
its expiry, the closed list of codes that says a new token would not help, and the sentence a turn ends with
are not built. What is built is the renewal they use, `Renewer::access_token` for a request and
`Renewer::replace_refused` for a token the host refused, which gives a token other than the refused one and
never the refused one back.

`verified-by: bravebot_skus::chatgpt::renew::a_token_with_more_than_a_minute_left_is_used_and_nothing_is_asked`
`verified-by: bravebot_skus::chatgpt::renew::a_token_within_a_minute_of_its_expiry_or_past_it_is_replaced`
`verified-by: bravebot_skus::chatgpt::renew::a_renewal_presents_the_stored_refresh_token_and_writes_what_it_is_given`
`verified-by: bravebot_skus::chatgpt::renew::a_reply_with_no_refresh_token_leaves_the_stored_one`
`verified-by: bravebot_skus::chatgpt::renew::no_stored_sign_in_is_not_signed_in`
`verified-by: bravebot_skus::chatgpt::renew::a_refresh_refused_as_ended_removes_the_sign_in`
`verified-by: bravebot_skus::chatgpt::renew::a_refresh_that_fails_otherwise_leaves_the_file_and_says_why`
`verified-by: bravebot_skus::chatgpt::renew::a_failed_refresh_sends_a_token_that_has_not_expired_and_never_one_that_has`
`verified-by: bravebot_skus::chatgpt::renew::a_refused_token_is_replaced_and_never_returned_when_the_refresh_fails`
`verified-by: bravebot_skus::chatgpt::renew::a_refused_token_another_process_already_replaced_is_not_replaced_again`
`verified-by: bravebot_skus::chatgpt::renew::a_token_made_after_the_refusal_is_sent_when_the_refresh_fails`
`verified-by: bravebot_skus::chatgpt::renew::a_renewal_made_while_waiting_for_the_claim_is_used_and_not_made_again`
`verified-by: bravebot_skus::chatgpt::renew::a_write_that_fails_is_reported_and_tried_again_before_the_next_request`
`verified-by: bravebot_skus::chatgpt::renew::tokens_held_in_memory_are_dropped_where_the_file_holds_another`
`verified-by: bravebot_skus::chatgpt::renew::an_ended_sign_in_removes_the_file_that_holds_the_token_this_process_replaced`
`verified-by: bravebot_skus::chatgpt::renew::each_failure_is_named_by_its_step_and_class_alone`
`verified-by: bravebot_skus::chatgpt::exchange::a_refresh_presents_the_refresh_token_under_the_client_and_the_resource`
`verified-by: bravebot_skus::chatgpt::exchange::only_the_recorded_code_at_a_refusing_status_ends_the_sign_in`
`verified-by: bravebot_skus::chatgpt::exchange::a_reply_without_a_refresh_token_keeps_the_one_presented`
`verified-by: bravebot_skus::chatgpt::exchange::the_exchange_refuses_a_reply_that_grants_no_refresh_token`

<a id="CHATGPT-8"></a>
### CHATGPT-8: a failure names its step and quotes nothing

Every refusal is one fixed sentence, chosen from the step that failed, the class of an HTTP status and,
for a refused refresh, whether its error code is the one that ends the sign-in. None holds text taken from a
reply, a request, a callback or a settings file. A request to either host that did not succeed reports one
of the backends spec's failure categories: refused, rate-limited, unavailable, incomplete or a transport
failure. The rest are these, each saying which it is:

- **Signing in again is the remedy,** and the sentence names `bravebot auth login chatgpt`: not signed in,
  the sign-in has ended, the service refused the token.
- **The sign-in file** cannot be read, cannot be written, or has nowhere to be kept on this platform, with the
  remedy the Leo store gives for a batch it cannot use.
- **The account may not be used this way.** A request is refused with one of the codes the exchange
  records as ending the use of a subscription: usage limit exceeded, user not eligible, capability or route
  not supported, invalid user, or the token's scope not authorized. The request is not repeated, and the
  sentence says the account cannot be used this way and does not name signing in again as the remedy.
- **The command stopped,** and says so: the listener could not start, the browser returned no code or no
  usable client, the ID token did not verify, the time bound passed, a `provider` entry named `chatgpt` is a
  gateway ([CHATGPT-9](#CHATGPT-9)). The entry's name in that sentence is the constant `chatgpt`. A reply
  that does not grant the scope `chatgpt.tokens.use.direct` ends the command in the same way, and the
  sentence says to sign in again and allow the program to use the subscription's tokens, or to use an API key
  with the `openai` gateway.
- **The command completed,** and says what it leaves: the account reaches no model, or no model can be
  offered for it because the listing could not be read and no list is available ([CHATGPT-9](#CHATGPT-9)).

The refusals of [CHATGPT-12](#CHATGPT-12), which are made before anything starts, are sentences of the
command and quote nothing.

No code, `state`, `nonce`, verifier, challenge, token, claim, client or host identifier, reply body, header
value or callback query reaches a trace, a log, an error message, a session record, a transcript, `doctor`,
`auth status`, the terminal or the environment of a program a turn starts. The one exception is the
authorization URL, which carries the `state`, the `nonce`, the challenge and the identifiers, is written to
the terminal once by [CHATGPT-13](#CHATGPT-13) and goes nowhere else. A test holds every secret the process held, in a run that signs in, refreshes and fails,
against each of those outputs, as the trail's test does for the credentials the agent holds, and leaves out
that one line.

**Not built:** the sentences of a refused refresh and of a refused model request, the completed command that
says the account reaches no model, and the test that holds every secret the process held against each output
of a run that signs in, refreshes and fails. The sentences of the sign-in are built.

**Why.** A message that quoted an authorization server's error would carry a value it chose to the person and
to anything that saves the message, and a log of a refresh would hold a token for as long as the log.
Choosing from a closed set also keeps [CHATGPT-5](#CHATGPT-5) true: what the agent branches on is a value
this crate computed from a step and a status class. The categories are the ones the other services'
failures already use, so a rate limit here is waited out and reported as a rate limit is elsewhere, and a
remedy that signing in again would not supply is not offered.

`verified-by: bravebot_skus::chatgpt::no_error_of_a_sign_in_holds_a_value_the_flow_handled`
`verified-by: bravebot_skus::chatgpt::a_failed_exchange_is_classed_by_its_status_and_quotes_nothing`
`verified-by: bravebot_skus::chatgpt::a_reply_that_does_not_allow_the_use_stores_nothing`
`verified-by: bravebot_skus::chatgpt::a_browser_that_never_returns_ends_the_sign_in_at_the_bound`
`verified-by: bravebot_skus::chatgpt::an_unreachable_server_and_a_reply_of_another_shape_are_reported_as_such`
`verified-by: bravebot_skus::chatgpt::exchange::a_status_is_classed_as_the_backends_classify_it`
`verified-by: bravebot_skus::chatgpt::account::a_refusal_of_a_malformed_file_quotes_none_of_it`
`verified-by: bravebot_cli::chatgpt::each_stop_of_the_sign_in_has_its_own_sentence_and_names_the_remedy_where_there_is_one`
`verified-by: bravebot_cli::chatgpt::a_failure_of_the_file_is_worded_with_its_detail_made_printable_and_the_remedy`
`verified-by: bravebot_cli::running::auth_login_chatgpt_is_refused_before_a_browser_opens_where_it_cannot_be_done`

<a id="CHATGPT-9"></a>
### CHATGPT-9: the account's models are offered under the identifier `chatgpt`

Once a usable sign-in is stored, the models the account may use appear in `/model` and in the desktop
picker as a roster beside the others. They take no model away from another roster, and without a sign-in
none is offered, since a name on the list promises that picking it works.

Each is named `chatgpt/<name>`, with the identifier ahead of the name the service knows the model by, as a
gateway's models are qualified by its id. A name without the identifier never selects this service, so the
bare `<name>` keeps its meaning: the `openai` gateway's model of that name, or Brave's. The text after the
identifier is what the request carries as the model. A row says which service answers it.

A `provider` entry with the id `chatgpt` that states an endpoint is a gateway, as it is today, and keeps the
name. While one exists no account model is offered and `auth login chatgpt` is refused, naming the entry,
since one name cannot select two services.

Where the roster comes from, in order:

1. An entry named `chatgpt` that states no endpoint is read for its `models` and for nothing else. Where it
   names models, those are what is offered and nothing is asked, as for a gateway. The person wrote that
   list, so the service refuses at the first request a model it names that the account cannot reach, and no
   listing is consulted to say so.
2. Where it names none, the serving host is asked for its listing, `GET /models` with the access token. The
   body is an object whose `models` key holds the rows, and a body of another shape is one that cannot be read.
   A row of the listing is offered if its `visibility` is `list` and `supported_in_api` is true, and a model
   the account cannot reach is not offered. The row's `slug` is the model name, `display_name` its label,
   `context_window` its window, `input_modalities` whether it takes images, and `supported_reasoning_levels`
   the effort levels it takes.
3. Where the listing cannot be read, because the request failed or its body is not the shape above, a
   compiled list of model names is offered instead, and the picker says the listing could not be read. A
   listing that is read and is empty offers nothing, and the compiled list is not used.

The window, the output ceiling and which models can call tools come from the entry or the listing as they do
for a gateway's, and the assumed low window applies where neither states one. The compiled list is a
constant of the source that names models only and takes nothing from a reply.

An account whose listing is empty, where the entry names no model, is signed in. The command completes,
says the account reaches no model, and the picker has no row for it. Nothing is invented for a tier nobody named.

**Why.** Where two services serve a model of one name they are billed differently, authenticate differently
and answer on different terms, so a bare name cannot say which one a person chose, and a remembered choice
would change service silently. An entry that already uses the id is not reinterpreted: refusing the sign-in
and naming the entry is a message, where taking the id over would send a working gateway's requests to
OpenAI's service with a person's subscription. The listing is asked first because the right list is a fact
about an account and a date, and a list that waited for a release would offer a model the account cannot
reach for weeks. It is the one source that says what an account may use, as [BACKEND-19](backends.md#BACKEND-19)
asks of a gateway. A compiled list stands in only where the listing cannot be read, because whether the
listing is dependable is not established ([What the exchange has to record](#what-the-exchange-has-to-record))
and a picker with no model is worse than one whose rows may be refused at the first request. It does not stand in for an empty listing, which is the service's answer.
Sign-in is about the account, so an account that reaches nothing today and something tomorrow is not signed
out by it. The alternative is a sign-in that fails for an account that reaches no model.

`verified-by: none`

<a id="CHATGPT-10"></a>
### CHATGPT-10: a request to the serving host is made in the Responses protocol and carries the token and the session's identifiers

The serving host is `https://api.openai.com/v1`, and it takes the Responses API. The request, the streamed
events, a tool call and its result, usage and cache figures and the effort level are those of the clause that
adds that protocol for any provider entry (#2994), and this service adds no protocol of its own. The effort
level maps to the service's reasoning field. Where the exchange shows the host also takes chat completions,
the aichat client may carry that as it carries a gateway's.

The access token is sent as a bearer token in `authorization`. The request carries three more headers,
`session-id`, `thread-id` and `x-client-request-id`, each holding the identifier of the session that makes
the request, which this program generated. The service derives its prompt-cache affinity from `session-id`.
The body is compressed with zstd and sent with `content-encoding: zstd`, since the service rejects an
uncompressed body over about 1 MiB and decodes zstd. Compaction of a conversation for this service is by
summary ([compaction.md](compaction.md)), as it is for every service, and the service offers no native
compaction to use instead. The effort level of a session is fixed when the session starts. Nothing else a reply, a model or a settings file says is
added to a request to this host: a gateway's options are carried into its requests, and this service takes
none. The sign-in stores no identifier of the account and no header value is taken from a reply.

**Why.** The service decides the protocol, and a protocol written for one service is a second copy of the
work for the next. A header a settings file could add would be a way to put a value, or another credential,
on a request this clause promises carries only the token and the session's identifiers. Headers built only
from constants and from values this program generated cannot carry a line break or a length a reply chose.
A body the service refuses for its size would fail a long conversation at the point a person has the most
to lose, so the encoding is not left to a setting.

`verified-by: none`

<a id="CHATGPT-11"></a>
### CHATGPT-11: logging out, status and doctor never print the sign-in

`bravebot auth logout chatgpt` removes `chatgpt-account.json` and says what would end the credential at
OpenAI, since the copy removed is not the credential. It is allowed in an incognito session and is not an
error where nothing is stored, as forgetting an import is.

`bravebot auth status chatgpt` prints one line. It says **signed in**, with the instant the access token
expires, where the file holds a sign-in, **not signed in**, with the command, where it does not, or
**unusable**, with the reason and the remedy, where the file cannot be read. An access token whose instant
has passed is still signed in, since the next request replaces it. The command probes nothing and writes
nothing. Its exit status is the one the other ways of that command give: 0 where the sign-in is usable. The
line holds no token, claim, identifier, email or account name.

`bravebot doctor` names the account as a held credential at its tier and says what would end it, and the
expiry. Where the file holds a sign-in it prints a `chatgpt` line with the instant the access token
expires, then the account every held credential is given ([CRED-25](credential-protection.md#CRED-25)): what
would end it, the tier, the binding, what revoking would not reach and the gate each drop failed. Where nothing
is stored it prints nothing about the sign-in, as it prints nothing for a gateway block that names no
credential. Where the file cannot be read it prints the line with the remedy and the run ends in failure, as a
file of gateway keys that cannot be read does, since the sign-in it held has stopped being used. The list
`auth login` prints says **signed in** on this way's line where the file holds a sign-in. Signing in again over
one asks first and replaces the tokens only on a yes.

**Why.** Each is an answer about whether the next request will work, and none needs the secret to give it.
A sign-in that OpenAI has ended is found out by a refresh and removed by it ([CHATGPT-7](#CHATGPT-7)), so
the file that is there is the one that has not been found to fail. An identifier or an email on a status
line is the one thing in a pasted report that says whose account it was.

`verified-by: bravebot_cli::running::auth_status_chatgpt_reports_the_expiry_and_nothing_that_could_sign_a_request`
`verified-by: bravebot_cli::running::auth_logout_chatgpt_forgets_the_sign_in_and_only_that_one`
`verified-by: bravebot_cli::chatgpt::a_stored_sign_in_is_reported_by_its_expiry_alone`
`verified-by: bravebot_cli::chatgpt::a_token_past_its_expiry_is_still_signed_in_and_nothing_stored_is_not`
`verified-by: bravebot_cli::chatgpt::a_file_that_cannot_be_used_is_unusable_and_no_place_to_keep_one_is_said_apart`
`verified-by: bravebot_cli::running::doctor_names_a_chatgpt_sign_in_by_its_expiry_and_prints_none_of_it`
`verified-by: bravebot_cli::chatgpt::a_stored_sign_in_needs_a_terminal_to_be_asked_about_and_a_first_one_does_not`

<a id="CHATGPT-12"></a>
### CHATGPT-12: signing in is refused where it cannot be done

`auth login chatgpt` is refused in an incognito session before the listener binds and before a browser
opens, as importing a subscription is refused there: the sign-in is a write to `~/.bravebot` that the mode
exists to decline. It is refused with the argument status where stderr is not a terminal, since the URL is shown there, and
says to run it at one. Where a sign-in is already stored the command asks before replacing it
([CHATGPT-11](#CHATGPT-11)), and is refused in the same way where stdin is not a terminal, since a run
nobody is watching cannot answer.

**Why.** Refusing the sign-in is the same argument as the import's: doing it and discarding the result
would leave a session that was signed in once and never again.

`verified-by: bravebot_cli::chatgpt::each_condition_that_cannot_be_answered_stops_the_command_with_its_own_ending`
`verified-by: bravebot_cli::chatgpt::a_stored_sign_in_needs_a_terminal_to_be_asked_about_and_a_first_one_does_not`
`verified-by: bravebot_cli::chatgpt::a_stored_sign_in_is_replaced_only_on_a_yes_and_a_first_one_asks_nothing`
`verified-by: bravebot_cli::chatgpt::the_mode_is_the_refusal_given_where_several_apply`
`verified-by: bravebot_cli::running::auth_login_chatgpt_is_refused_before_a_browser_opens_where_it_cannot_be_done`

<a id="CHATGPT-13"></a>
### CHATGPT-13: the URL is always printed, so a machine with no browser can finish the sign-in elsewhere

The authorization URL is written to stderr, line by line as it is made, before the browser is asked to open
it, whether or not one can be opened. It is followed by one line naming the port the listener is bound to.
The command waits for the callback in every case. A person with no browser on the machine opens the URL in
one elsewhere.

That works where the browser can reach this machine's loopback port: it is the same machine, or the port is
forwarded, and the line says which port. Where it cannot, the redirect fails in the browser, and the command
ends at the time bound with the sentence of [CHATGPT-8](#CHATGPT-8) that says so. No device-code route is
used, since the one that exists is for another program's client.

The browser is opened by the platform's own opener, started with the URL as one argument and no shell
between them: `open` on macOS, `xdg-open` on Linux and `rundll32` with `url.dll,FileProtocolHandler` on
Windows, where `start` is a shell builtin. It is started as the sandboxing spec says a program is started
that bravebot looks up by name, resolved first and with the variables that spec removes from what an agent
passes on left out. The opener failing is not the sign-in failing, and the command carries on waiting.

**Why.** A sign-in that printed the URL only when the opener failed would show nothing on a machine where
the opener exits successfully and opens nothing, which a machine reached over SSH may do. A shell between
the program and the URL would read the `&` the query holds. The URL holds the `state`, the challenge and the
identifiers, and the person needs it, so it is the one line [CHATGPT-8](#CHATGPT-8) lets reach the terminal.

`verified-by: bravebot_cli::chatgpt::the_address_and_the_port_are_shown_before_the_browser_is_asked_for`
`verified-by: bravebot_cli::chatgpt::the_address_is_one_argument_of_the_platforms_own_opener`
`verified-by: bravebot_cli::chatgpt::the_opener_is_not_handed_this_agents_own_credentials`
`verified-by: bravebot_cli::chatgpt::a_browser_opener_in_a_directory_a_stage_may_write_is_not_the_one_started`
`verified-by: bravebot_cli::chatgpt::a_program_the_opener_starts_by_name_is_never_one_a_stage_may_have_written`
`verified-by: bravebot_skus::chatgpt::the_announcement_names_the_port_the_listener_is_already_bound_to`
`verified-by: bravebot_skus::chatgpt::a_browser_that_never_returns_ends_the_sign_in_at_the_bound`

<a id="CHATGPT-14"></a>
### CHATGPT-14: a refresh is still written in an incognito session

A stored sign-in is spent in an incognito session as in any other. When the access token is replaced
([CHATGPT-7](#CHATGPT-7)), the file is written, as the incognito spec writes the markers of a credential
that was spent, and a sign-in that has ended is removed from it. What the file ends up holding is the tokens
of a sign-in made before the session, and nothing about which project or which prompts.

**Why.** A refresh that was not written would end the sign-in whenever the server replaces the refresh
token on use, and the person would find out at the first turn of the next session.

**Not built:** a session that asks for a token, so no test drives an incognito session through a renewal.
What is built is the renewal, which takes no mode and writes the file whatever the caller is doing.

`verified-by: bravebot_skus::chatgpt::renew::a_renewal_presents_the_stored_refresh_token_and_writes_what_it_is_given`
`verified-by: bravebot_skus::chatgpt::renew::a_write_that_fails_is_reported_and_tried_again_before_the_next_request`

## What is built

The sign-in is the `chatgpt` module of `bravebot-skus` and the command is `crates/cli/src/chatgpt.rs`.
[LAYER-1](layering.md#LAYER-1)'s row for `bravebot-skus` covers the module, which needs no dependency the
crate does not hold and depends on no crate of this workspace, so [CHATGPT-5](#CHATGPT-5)'s first condition
holds by reading its manifest.

- **Built whole:** [CHATGPT-3](#CHATGPT-3), [CHATGPT-4](#CHATGPT-4), [CHATGPT-6](#CHATGPT-6),
  [CHATGPT-11](#CHATGPT-11), [CHATGPT-12](#CHATGPT-12) and [CHATGPT-13](#CHATGPT-13).
- **Built in part,** each naming what is not: [CHATGPT-1](#CHATGPT-1), [CHATGPT-2](#CHATGPT-2) and
  [CHATGPT-8](#CHATGPT-8).
- **Built in part,** the replacing of an access token, which is the `renew` module and has no caller yet:
  [CHATGPT-7](#CHATGPT-7) and [CHATGPT-14](#CHATGPT-14), each naming what is not.
- **Not built:** [CHATGPT-9](#CHATGPT-9), the account's models in the picker; [CHATGPT-10](#CHATGPT-10), requests to the
  serving host, which wait on the Responses protocol of #2994 and on a zstd encoder; and the verification
  of the ID token of [CHATGPT-5](#CHATGPT-5).

A sign-in made now is stored and can be renewed, and is not yet used by a turn: the command says so.

## Decisions left open

These are a person's to decide, and no test can settle them.

- **Whether OpenAI's terms allow this.** The sign-in registers a client of its own, named `bravebot`, and
  presents no other program's identifier. Whether ChatGPT's terms allow a third-party program to use a
  subscription this way, and whether OpenAI's registration accepts this program, has to be decided before
  this ships. If it does not, this should not land as written.
- **Verifying the ID token.** [CHATGPT-5](#CHATGPT-5) verifies it with RS256 against OpenAI's key set, as
  the program this is read from does. That needs a signature verifier and a request for the key set. Whether
  the tree already holds a verifier that serves, or one is added, is a decision about a dependency
  ([DEP-001](../best-practices/dependencies.md#DEP-001)). The alternative is to read no ID token, which
  drops the `nonce` and the audience check and leaves the code exchange and PKCE to tie the sign-in to this
  process. The command is built on that alternative until this is decided: the `nonce` is sent and not
  checked, and the reply's `id_token` is dropped unread. The tree holds no RS256 verifier, so the other
  choice adds a dependency.
- **Compressing the body.** [CHATGPT-10](#CHATGPT-10) sends zstd. The tree may hold no encoder, and adding
  one is a decision about a dependency in the same way.
- **The compiled list.** [CHATGPT-9](#CHATGPT-9) offers one where the listing cannot be read. Its contents
  are a constant a release changes. The alternative is to offer nothing in that case and say to name the
  models in the `chatgpt` entry.

## What the exchange has to record

A sign-in made with a real ChatGPT Plus or Pro account, with the requests it makes observed, has to
establish each of the following before a clause states it as observed. What is recorded is a name, a shape,
a status, a duration or a host. A token, code, state, nonce, challenge, verifier, client or host identifier
or claim value is never recorded, in this repository or in the tracker.

**Observed, in a sign-in made on 2026-10-23 with a real ChatGPT account on this program's own build:**

- The authorization endpoint and token endpoint on `auth.openai.com`, with the paths of
  [CHATGPT-2](#CHATGPT-2), and the sign-in that ran on them: the browser was sent to the loopback listener on
  a port the operating system chose, and the exchange completed.
- That the authorization server registers a client for the placeholder client and `agent_name_hint`: a
  `client_id` came back on the callback and was stored, and the refresh that followed accepted it.
- The scopes and the `resource`, `nonce` and `ext_agent_host_id` parameters were accepted, and the reply's
  `scope` held `chatgpt.tokens.use.direct`, since the command refuses a reply that lacks it and completed.
- The field names of the code exchange and of the refresh, both form encoded. A reply to a refresh has a new
  `access_token` and `refresh_token`, and an expiry 3600 seconds out, which is `expires_in` or the default
  the renewal assumes where it is absent. The two are not told apart.
- **A refresh replaces the refresh token.** The one stored before a refresh is not the one stored after it,
  and the client and host identifiers are unchanged. The claim and the write-before-use of
  [CHATGPT-7](#CHATGPT-7) are therefore required and not a precaution.
- The listing, `GET /models` on `https://api.openai.com/v1` with the access token as the only header, is
  answered 200. The body is an object with one key, `models`, holding the rows, and is about half a megabyte
  for nine rows, since a row carries long instruction text the picker does not read. Each row has `slug`,
  `display_name`, `visibility` (`list` or `hide`), `supported_in_api`, `context_window`, `input_modalities`
  (a list of words) and `supported_reasoning_levels` (a list of objects with an `effort` and a
  `description`). The account's listing had nine rows and seven of them were `list`. The efforts offered
  go beyond `low`, `medium` and `high`, up to `max` and `ultra` on some rows.

**Stated by the clauses from opencode's source, to confirm:**

- The key set on `auth.openai.com`, with its path in [CHATGPT-2](#CHATGPT-2).
- That the authorization server accepts a redirect to `http://127.0.0.1:<port>/auth/callback` for any port:
  one port was tried.
- The `id_token` field of the code exchange's reply, which the command drops unread.
- The ID token's algorithm, issuer, audience, required claims and the nonce match.
- The serving host's Responses protocol, the three session headers, the zstd body, and that there is no
  native compaction and no change of effort in a session. The bearer token is accepted by the listing.
- The error codes that end the use of a subscription: `subscription_sharing_usage_limit_exceeded`,
  `subscription_sharing_user_not_eligible`, `subscription_sharing_unsupported_capability`,
  `subscription_sharing_route_not_supported`, `subscription_sharing_invalid_user`,
  `chatpass_v2_scope_not_authorized` and `chatpass_v2_invalid_authorization_context`.

**Not in any source this spec was written from:**

- What a refused refresh and a refused access token return: the status and the error code. The renewal reads
  `invalid_grant` at a 400 or 401 as ending the sign-in, from RFC 6749 and not from this server.
- Whether a request to the token endpoint is ever answered with a redirect.
- What an account with no subscription reaches.
- Whether an access token minted before a revocation survives it, and whether a sender-constrained form is
  offered.
- Whether the listing is dependable, and what a listing that includes a model the account cannot reach does
  at the first request.
- Whether OpenAI limits the clients registered for one host identifier, and what a second sign-in on a
  machine that has signed out registers.

## What this changes elsewhere

These are the places a built spec says something this spec would need to extend, each decided when the
work lands, in the same commit as the code.

- **Backends.** [BACKEND-3](backends.md#BACKEND-3) and [BACKEND-18](backends.md#BACKEND-18) gain the
  `chatgpt/` identifier. [BACKEND-9](backends.md#BACKEND-9) gains a note that this service starts no
  sign-in. [BACKEND-39](backends.md#BACKEND-39) names three ways to configure a service, and a stored
  ChatGPT sign-in is a fourth. [BACKEND-13](backends.md#BACKEND-13), [BACKEND-17](backends.md#BACKEND-17)
  and [BACKEND-5](backends.md#BACKEND-5) each gain the case of [CHATGPT-9](#CHATGPT-9): an entry named
  `chatgpt` that states no endpoint is read for `models`, where today an entry with no endpoint configures
  no service. [BACKEND-19](backends.md#BACKEND-19) gains this service's listing and its absence. The cases
  of [CHATGPT-10](#CHATGPT-10) land in [BACKEND-3](backends.md#BACKEND-3) to
  [BACKEND-6](backends.md#BACKEND-6), [BACKEND-20](backends.md#BACKEND-20) to
  [BACKEND-22](backends.md#BACKEND-22), [BACKEND-31](backends.md#BACKEND-31) and
  [BACKEND-52](backends.md#BACKEND-52) with the Responses protocol. [BACKEND-20](backends.md#BACKEND-20)
  also gains the case that the effort level of a session is fixed when it starts, and the aichat client
  gains a zstd body encoding.
- **The command line.** [CLI-18](cli.md#CLI-18) lists five ways and states `logout chatgpt`.
  [CLI-23](cli.md#CLI-23) takes `chatgpt` as a way.
- **Incognito.** [INCOG-7](incognito.md#INCOG-7) refuses the sign-in, and
  [INCOG-8](incognito.md#INCOG-8) lists the refresh among what is still written.
- **Credential protection.** [CRED-14](credential-protection.md#CRED-14) and the list of what is never read
  at its strict form add `chatgpt-account.json`, and [CRED-25](credential-protection.md#CRED-25)'s `doctor`
  account gains the credential.
- **Sandboxing.** [SANDBOX-30](sandboxing.md#SANDBOX-30)'s list of programs started by name gains the
  browser opener of [CHATGPT-13](#CHATGPT-13).
- **Layering.** [LAYER-1](layering.md#LAYER-1)'s row for `bravebot-skus` says the crate also runs this
  sign-in. The flow is auth only, holds no workspace content and no labelled value, and depends on no crate of
  this workspace. The browser is started by `bravebot-cli`, which depends on `bravebot-sandbox`. A crate of its
  own is needed if the flow comes to need the secret type of `bravebot-config` or the clearing of buffers in
  `bravebot-sandbox`, which `bravebot-skus` spells for itself. [CHATGPT-5](#CHATGPT-5) rests on the dependency
  set either way.
- **Network egress.** [network-egress.md](network-egress.md) names no host for any service, so none is added,
  and the key set is fetched from the authorization server's host without a credential. The crate keeps a client of its own as `bravebot-skus` does, because a request here carries no labelled
  value for the gate in `bravebot-net` to judge, so the register in its Known costs gains the crate, and the
  client is configured to follow no redirect, which the register records as undecided for `bravebot-skus`.
  The register's cost about a hop to any https host gains an exception: a request to the serving host that
  carries the access token follows no redirect, since a person did not choose that host. Its scope is
  requests that leave this process, so the listener, which receives one, is governed by
  [CHATGPT-4](#CHATGPT-4).
- **Labels.** Nothing changes. [CHATGPT-5](#CHATGPT-5) shows the flow holds the rule, and no exception is
  added.

## Known costs

- The refresh token is a bearer secret in a file for as long as the account keeps the sign-in. Anything
  running as the person can read it, as it can read the Leo credentials and the gateway keys. Nothing asks
  for a form that binds it to this machine.
- The callback puts a `code` and a `state` in the address bar and history of the person's own browser. The
  `code` is single use and exchanges only with the verifier this process holds.
- The `state` is in the argument list of the program that opens the browser. A local program that reads it
  can send the listener a callback with no `code` and end the sign-in. It can end one as well by holding the
  port first, and it cannot complete one.
- The ID token is verified against a key set fetched from the authorization server at the sign-in
  ([CHATGPT-5](#CHATGPT-5)), so a sign-in with no route to that host fails, and the verifier is code that
  parses a token somebody else made.
- A sign-in ends at OpenAI and this program learns of it at the next refresh, which is the first turn that
  needs one. A turn can also be refused once for a token that a clock running behind made look valid.
- A process that rotated the refresh token and could not write the file holds the only good copy in
  memory. If another process finds the file's refresh token dead and removes it, and the first exits before
  its write succeeds, the sign-in is gone and the person signs in again.
- A person on a machine with no browser forwards the listener's port, which changes at every sign-in, and
  no device-code route is available to this program.
- The compiled list ([CHATGPT-9](#CHATGPT-9)) can name a model the account cannot reach, which the service
  refuses at the first request.
- One account is signed in at a time on one machine, as one subscription is imported at a time.
- A person whose gateway entry already has the id `chatgpt` renames it to sign in.
- The sign-in is stored and renewed and no request is sent with it, so a sign-in made now is a file that
  nothing reads until the clauses that use it are built.
- The code that says a refresh token is no longer valid is RFC 6749's and has not been seen from this
  server. A server that ends a sign-in with another code leaves the file in place, and each later turn tries
  the dead refresh token and is told the refresh failed, until the person signs in again.
- The ID token is not verified, so the `nonce` and the audience are not checked
  ([Decisions left open](#decisions-left-open)).
- Whether a third-party program may use a subscription this way is not this spec's to settle
  ([Decisions left open](#decisions-left-open)). The design assumes a person is allowed to, and an answer of
  no ends it.
