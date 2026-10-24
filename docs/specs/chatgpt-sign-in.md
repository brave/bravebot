---
id: CHATGPT
title: Signing in with a ChatGPT account
status: proposed
governs:
  - crates/cli/src/auth.rs
  - crates/config/src/provider.rs
  - crates/agent/src/backend.rs
  - crates/agent/src/chatgpt.rs
  - crates/aichat/src/lib.rs
  - crates/aichat/src/responses.rs
  - crates/aichat/src/models.rs
  - crates/tui/src/app/models.rs
  - crates/ui-bridge/src/models.rs
  - crates/ui-bridge/src/bridge.rs
  - crates/session/src/sessions.rs
  - ui/src/renderer/failure.ts
  - crates/cli/src/chatgpt.rs
  - crates/skus/src/chatgpt/mod.rs
  - crates/skus/src/chatgpt/account.rs
  - crates/skus/src/chatgpt/authorize.rs
  - crates/skus/src/chatgpt/encoding.rs
  - crates/skus/src/chatgpt/exchange.rs
  - crates/skus/src/chatgpt/id_token.rs
  - crates/skus/src/chatgpt/listener.rs
  - crates/skus/src/chatgpt/renew.rs
  - crates/skus/src/lib.rs
  - crates/config/src/lib.rs
  - crates/agent/src/turn/task.rs
  - crates/agent/src/outcome.rs
  - crates/agent/src/turn.rs
  - crates/aichat/src/responses/protocol.rs
  - crates/tui/src/status.rs
  - crates/tui/src/panel.rs
  - crates/tui/src/state/settings.rs
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
them, and `auth status` and the list of ways report them. [What is built](#what-is-built) says where the code
is and that every clause is built whole. [What the exchange has to record](#what-the-exchange-has-to-record)
lists what a sign-in made with a real account has observed and what it has not. The points where this spec
changes another spec are named in [What this changes elsewhere](#what-this-changes-elsewhere), and each of
those specs holds the change.

The hosts, parameters, field names and headers the clauses state are read from opencode's source
(`packages/core/src/plugin/provider/chatgpt.ts`, at commit `ac68d6a929`), which is another program's code.
Each is a value to confirm with a sign-in unless the list below marks it observed, and [What the exchange has to record](#what-the-exchange-has-to-record)
lists them beside what no source states. Whether OpenAI's terms allow a program to use a subscription this
way, and whether OpenAI registers a client for this program, are
[decisions left open](#decisions-left-open). The command is built on the values as stated, and a release
that carries it waits on the first of those decisions. A request with the sign-in is sent in the Responses
protocol of [BACKEND-61](backends.md#BACKEND-61), as [CHATGPT-10](#CHATGPT-10) states.

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
`verified-by: bravebot_aichat::client::an_account_with_no_sign_in_sends_nothing`
`verified-by: bravebot_cli::running::a_turn_on_a_chatgpt_model_with_no_sign_in_ends_naming_the_command_and_stores_nothing`

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
name. A test points the flow at a server of its own through a seam that exists only in a debug build with
the `test-hosts` feature of `bravebot-skus` and of `bravebot-config`, under which the variable
`BRAVEBOT_TEST_CHATGPT_STAND_IN` names one server that stands in for the four destinations. Only the
`[dev-dependencies]` of `bravebot-cli` enable it, so the binary `cargo test` builds for that crate's tests
has it and a release build does not. A build without the feature holds no code that reads the variable, and
a release build with it, by `--all-features` or by a manifest, does not compile.

The access token is sent to the serving host and to no other. The refresh token and the code are sent to
the token endpoint and to no other. The request for the key set carries no credential. A request to any of
the four follows no redirect, whichever client makes it, the one in the crate that signs in or the one that
sends a model request: a redirect in answer to it is a failure. A gateway key, including the `openai` gateway's, is never sent to these hosts,
and the tokens of this account are never sent to a gateway.

A model of this service is answered with the stored sign-in and with nothing else. `OPENAI_API_KEY`, a
gateway key and any other variable are not looked for.

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
`verified-by: bravebot_skus::chatgpt::a_key_set_that_cannot_be_had_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_cli::layering::a_test_seam_is_enabled_by_dev_dependencies_alone`
`verified-by: bravebot_cli::layering::the_seam_reader_finds_a_seam_wherever_a_build_that_ships_reads_it`
`verified-by: bravebot_config::provider::the_chatgpt_account_is_reached_at_the_serving_host_and_is_given_no_key_of_its_own`
`verified-by: bravebot_net::lib::a_request_refusing_redirects_is_answered_with_the_redirects_status_and_goes_no_further`
`verified-by: bravebot_aichat::client::a_redirect_in_answer_to_an_account_request_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_aichat::client::an_account_request_carries_the_token_the_three_session_headers_and_the_zstd_encoding_and_nothing_a_gateway_would`

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
token, which has to name it as its audience ([CHATGPT-5](#CHATGPT-5)).

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

- **A branch on untrusted bytes.** The comparisons are the `state` against the value generated here, a
  reply's status and the fields the next step needs, and the ID token's header, signature and five claims
  against the key set and the values this sign-in sent. They decide this program's own sign-in. What crosses
  into the driver is a token and a category, which is the class of a reply's status, as the failure
  categories of the backends spec are for the reply of any other service. A refused refresh adds one value,
  that the refresh token is no longer valid, chosen by comparing one field of the error body with `invalid_grant`
  ([CHATGPT-7](#CHATGPT-7)). A refused model request adds two more, that the account has reached its usage limit
  and that it may not be used this way, chosen by comparing the error code of the body with a closed list
  of codes the exchange records ([CHATGPT-8](#CHATGPT-8)); that comparison is made in the client that sends the request, which is not the
  driver, and the egress crate hands the client the whole body of the refusal under the label of its
  request and reads none of it ([NET-12](network-egress.md#NET-12)). A turn already branches on the categories, and none is content anyone authored. Which of
  these values a failure was is carried to the terminal and the desktop beside its category, where it
  chooses a sentence ([CHATGPT-8](#CHATGPT-8)), and the driver adds no comparison of its own for it. The
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
   [credential-protection.md](credential-protection.md) describes, a value from the closed set of
   [CHATGPT-8](#CHATGPT-8), which for a refusal that names the account holds the status of the reply and
   the wait it asked for as numbers, the wait bounded as any other is, and whether the file took a
   renewal, which is the result of a write on this machine. No string taken from a reply is part of any
   of them. The driver copies the numbers into the line a person reads and decides nothing from them.
3. The ID token is verified once, at the sign-in, before anything is stored, and no claim of it is
   stored, sent or displayed. A code exchange whose reply holds no `id_token`, or one that is not a
   string, ends the sign-in as a token that did not verify, so no sign-in is stored unverified. A token
   longer than 16 KiB is refused before it is decoded. Its header names RS256 and nothing that has to be
   understood: `none`, an HMAC and every other algorithm are refused, and so is a header with `crit`. The
   key is the only key of the set with the header's `kid`, and it is an RSA key a signature may be checked
   with: `kty` is `RSA`, and `use`, `alg` and `key_ops`, where present, say `sig`, `RS256` and `verify`.
   A set that holds two keys under one `kid` has none that is the key.
   The key set is fetched from the host compiled in for it ([CHATGPT-2](#CHATGPT-2)) with no credential,
   following no redirect, and a reply over 64 KiB is refused. The signature is checked with RS256 against
   that key's modulus and exponent, and then the issuer (the constant `https://auth.openai.com`), the
   audience (a string equal to the client of the sign-in, or a list holding it), the expiry (later than
   the clock), the nonce (the one sent, compared in constant time) and the subject (a string that is not
   empty) are compared. No other claim is read. The decoded header and claims are cleared when they are
   dropped. The access token's expiry is read from the `expires_in` field of the reply, not from the
   token, and is compared with the clock. Claims such as an organisation's name are text somebody else
   chose.

No exception is added to [labels.md](labels.md). The counts it pins for `Labelled::new` and
`Policy::decode_transport` move by the one place a refusal's body is read and the one that hands it back,
and by the tests that exercise them.

**Why.** The rule protects the planner and the driver from content an attacker could write. A callback is
the one place here where bytes from outside arrive, and whoever wrote them could reach a loopback port: any
program on the machine, or a page in the browser. Nothing they could write reaches a turn, because the
only things read from the callback are a value compared with one generated here, and a `code` and a client
identifier handed to the authorization server, which accepts the pair only if it issued the `code` to that
client for this verifier. A reply from that server is the other place, and what leaves the crate from it is a
token that is never read, or a category computed from a status. The ID token and the key set are read for
whether the token verified, and what leaves the crate is that answer and no claim.

`verified-by: bravebot_skus::chatgpt::an_id_token_that_does_not_verify_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_skus::chatgpt::an_id_token_whose_audience_lists_the_client_among_others_signs_in`
`verified-by: bravebot_skus::chatgpt::a_key_set_that_cannot_be_had_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_skus::chatgpt::a_first_sign_in_exchanges_the_code_under_the_issued_client_and_stores_the_account`
`verified-by: bravebot_skus::chatgpt::id_token::only_an_rsa_key_for_rs256_signatures_named_by_the_kid_is_used`
`verified-by: bravebot_skus::chatgpt::encoding::base64url_reads_back_what_it_writes_and_nothing_else`

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
say the account has reached its usage limit or may not be used this way, and a new token does not change
that. The code is the `error.code` of
the refusal's body, compared as a whole with the list, and a body that holds none, holds another or cannot be
read leaves the refusal as its status alone. A second refusal ends the
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
backends spec's failure categories it was. A write of the reply that fails is reported to the person, once,
as a notice of the turn, in a sentence that names no path and quotes nothing the file or the system said. The
notice is given after the request that renewed it, and when the turn ends if it has not been given, so a
renewal made by a call that is not a round's request, such as the goal check or a processor, is reported in
the turn it was made in. A renewal made outside a turn, for a picker's listing, the sign-in command or a
compaction at rest, is reported by the next turn that makes a request. The new tokens are used from memory
for the rest of the process, and the process tries the write again before each later request, under the claim,
and only if the file still holds the token this process replaced. A retry that fails again is not reported
again, and one that succeeds clears the report, so a failure after it is reported. Where the file holds
another token, or none, the tokens in memory are dropped.

**Why.** An authorization server may replace the refresh token each time it is used, and then the copy a
second process read before the first's reply is dead. A session, a background run and a one-shot run on one
machine present it together as a matter of course, and the one that loses ends the sign-in for all of them.
The claim is held across a network round trip, so it has an age longer than that trip's deadline: the Leo
claim is judged abandoned after seconds because its hold is one local read and write. The reply is written
first because a process that stops between receiving it and writing it has spent a refresh token it cannot
present again. This server replaces the refresh token on use, so the claim and the early write are
required here. A server that did not replace it would lose nothing to them. Removing the file
on an ended sign-in leaves `auth status` and the picker saying what is true, and stops every later turn
from presenting a token that cannot work.

A turn asks the renewal before every request (`Renewer::access_token`) and after a refusal of the token
(`Renewer::replace_refused`), which gives a token other than the refused one and never the refused one back.
Only a refusal with the status 401 is a refusal of the token, and one whose body names a code of the closed
list ends the turn without a replacement.

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
`verified-by: bravebot_aichat::client::an_account_asks_for_its_token_before_every_request_and_sends_the_one_it_is_given`
`verified-by: bravebot_aichat::client::a_token_the_host_refuses_is_replaced_once_and_the_request_is_sent_again_with_the_new_one`
`verified-by: bravebot_aichat::client::a_second_refusal_or_a_replacement_that_cannot_be_made_ends_the_turn_in_the_sign_ins_words`
`verified-by: bravebot_aichat::client::a_refusal_that_names_no_listed_code_is_replaced_and_sent_again_as_before`
`verified-by: bravebot_agent::chatgpt::each_failed_step_of_a_renewal_is_its_own_failure_and_only_an_ended_account_is_ended`
`verified-by: bravebot_agent::chatgpt::a_renewal_the_file_could_not_take_is_said_once_and_again_only_after_a_write_succeeds`
`verified-by: bravebot_agent::chatgpt::the_report_of_an_unwritten_renewal_is_what_is_noted_and_the_token_is_used_either_way`
`verified-by: bravebot_agent::chatgpt::the_sentence_for_a_renewal_the_file_could_not_take_quotes_nothing_and_names_the_remedy`
`verified-by: bravebot_agent::chatgpt_renewal_notice::a_turn_says_a_renewal_the_file_could_not_take_where_it_was_made_and_once`

<a id="CHATGPT-8"></a>
### CHATGPT-8: a failure names its step and quotes nothing

Every refusal is one fixed sentence, chosen from the step that failed, the class of an HTTP status and,
for a refused refresh, whether its error code is the one that ends the sign-in, and for a refused model
request, whether its error code is one of a closed list. None holds text taken from a reply, a request, a
callback or a settings file. A request to either host that did not succeed reports one
of the backends spec's failure categories: refused, rate-limited, unavailable, incomplete or a transport
failure. The rest are these, each saying which it is:

- **Signing in again is the remedy,** and the sentence names `bravebot auth login chatgpt`: not signed in,
  the sign-in has ended, the service refused the token.
- **The sign-in file** cannot be read, cannot be written, or has nowhere to be kept on this platform, with the
  remedy the Leo store gives for a batch it cannot use.
- **The account has reached its usage limit, or may not be used this way.** A request is refused with one of
  the codes the exchange records as ending the use of a subscription, in the `error.code` of the refusal's
  body, whatever its status, or of a `response.failed` event or an `error` event of the stream, where it is
  `code`. The request is not repeated and no token is replaced. A refusal that came in a reply keeps the
  status of that reply and, at a 429, the wait its `Retry-After` asked for, as numbers and bounded as any
  other wait is. A refusal reported in an event of the stream arrives in a reply that was answered 200 and
  carries neither. `subscription_sharing_usage_limit_exceeded`
  has a sentence of its own, which says the account has reached its usage limit. The other six (user not
  eligible, capability or route not supported, invalid user, the token's scope or authorization context not
  authorized) have one sentence that says the account may not be used this way. It names no cause, since the
  six do not say which, and does not name signing in again as the remedy. Both are a refusal among the
  categories, which no fallback model is used for: the account answers for every model it serves, so the
  fallback would be refused the same way.
- **The command stopped,** and says so: the listener could not start, the browser returned no code or no
  usable client, the reply held no ID token or one that did not verify, the key set could not be fetched (in
  the category of that request), the time bound passed, a `provider` entry named `chatgpt` is a
  gateway ([CHATGPT-9](#CHATGPT-9)). The entry's name in that sentence is the constant `chatgpt`. A reply
  that does not grant the scope `chatgpt.tokens.use.direct` ends the command in the same way, and the
  sentence says to sign in again and allow the program to use the subscription's tokens, or to use an API key
  with the `openai` gateway.
- **The command completed,** and says what it leaves: the account reaches no model, or the listing could not
  be read and the picker offers the compiled list in its place, which may name a model the account cannot
  reach ([CHATGPT-9](#CHATGPT-9)), or no model can be offered for it because the sign-in file could not be
  read where an entry names the models, or because the service refused the listing in a code of the closed
  list, in that code's sentence. It says the sign-in is stored before it asks the listing, so a service slow
  to answer leaves a person knowing it was kept, and the ending is a success whatever the listing says.

The refusals of [CHATGPT-12](#CHATGPT-12), which are made before anything starts, are sentences of the
command and quote nothing.

**What a front end is given.** The diagnosis of a failed turn carries which of the failures above it was,
beside its category ([BACKEND-37](backends.md#BACKEND-37)), and the failure the desktop is sent carries its
fixed name as `account`, which is `null` for a failure that did not come from the account. The value is a
step of this clause or the refusal chosen above, and no string from a reply is in it. A refusal's status is
in the diagnosis and in the desktop's `status`, and the wait it asked for is in the diagnosis. The planner
is told the category and the request count, as for every failure ([TURN-13](turns.md#TURN-13)), and not
which failure of the account it was, nor a status or a wait.

The one-shot run reports the sentences above as the error of the failed turn. The terminal says the
sentence of the value from its catalog, worded as that report words it, and the record of a session keeps
that line in either front end. It is the whole reason, so no count of tries follows it, since what it says
to do ends it. A refusal that came in a reply says the wait it asked for in its sentence, in minutes rounded
up, and its status after it: "OpenAI says this ChatGPT account has reached its usage limit, and the service
asked for a wait of about 10 minutes (HTTP 429)". A refusal reported in an event of the stream carries
neither and is the sentence alone, as does every other failure of the account. A failure that carries no
value is worded from its category, with its status and count.

The desktop is sent the terminal's sentence for the value, capitalised, as `accountMessage` beside the
name, and draws it as the description of one card in `ui/src/renderer/failure.ts`. The window keeps no
table of the sentences, so the sentence that sends a person to sign in names the command the terminal
names ([CHATGPT-1](#CHATGPT-1)). A failure with no sentence is drawn from its category.

No code, `state`, `nonce`, verifier, challenge, token, claim, client or host identifier, reply body, header
value or callback query reaches a trace, a log, an error message, a session record, a transcript, `doctor`,
`auth status`, the terminal or the environment of a program a turn starts. The one exception is the
authorization URL, which carries the `state`, the `nonce`, the challenge and the identifiers, is written to
the terminal once by [CHATGPT-13](#CHATGPT-13) and goes nowhere else. A test holds every secret the process held, in a run that signs in, refreshes and fails,
against each of those outputs, as the trail's test does for the credentials the agent holds, and leaves out
that one line.

The test runs the built binary against a stand-in through the seam of [CHATGPT-2](#CHATGPT-2): the sign-in in
a terminal, `auth status`, `doctor`, a turn that renews a refused token and is answered, and a turn whose
refresh is refused as no longer valid, every refusal repeating every value the stand-in has seen. It searches
each command's streams under `--trace` and the debug log, every file left in the home, and the environment a
hook started by each turn writes there. The desktop window is not one of them: what the bridge says of a
failed listing is pinned by
`bravebot_ui_bridge::models::the_accounts_models_join_the_roster_and_a_failed_listing_adds_the_compiled_list_and_a_warning`.
The test runs on Linux and macOS, where `script` gives the sign-in a terminal, and not on Windows.

**Why.** A message that quoted an authorization server's error would carry a value it chose to the person and
to anything that saves the message, and a log of a refresh would hold a token for as long as the log.
Choosing from a closed set also keeps [CHATGPT-5](#CHATGPT-5) true: what the agent branches on is a value
this crate computed from a step and a status class, or one of the values the client chose by comparing a
refusal's code with a closed list. The categories are the ones the other services' failures already use, so
a rate limit by status here is waited out and reported as a rate limit is elsewhere, and a remedy that
signing in again would not supply is not offered. A usage limit named by a code is not waited out, since
nothing a second request or a wait of this program's does changes it.

`verified-by: bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_an_output_of_a_run_that_signs_in_renews_and_fails`
`verified-by: bravebot_skus::chatgpt::no_error_of_a_sign_in_holds_a_value_the_flow_handled`
`verified-by: bravebot_skus::chatgpt::an_id_token_that_does_not_verify_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_skus::chatgpt::a_key_set_that_cannot_be_had_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_skus::chatgpt::a_failed_exchange_is_classed_by_its_status_and_quotes_nothing`
`verified-by: bravebot_skus::chatgpt::a_reply_that_does_not_allow_the_use_stores_nothing`
`verified-by: bravebot_skus::chatgpt::a_browser_that_never_returns_ends_the_sign_in_at_the_bound`
`verified-by: bravebot_skus::chatgpt::an_unreachable_server_and_a_reply_of_another_shape_are_reported_as_such`
`verified-by: bravebot_skus::chatgpt::exchange::a_status_is_classed_as_the_backends_classify_it`
`verified-by: bravebot_skus::chatgpt::account::a_refusal_of_a_malformed_file_quotes_none_of_it`
`verified-by: bravebot_cli::chatgpt::each_stop_of_the_sign_in_has_its_own_sentence_and_names_the_remedy_where_there_is_one`
`verified-by: bravebot_cli::chatgpt::a_failure_of_the_file_is_worded_with_its_detail_made_printable_and_the_remedy`
`verified-by: bravebot_cli::chatgpt::each_answer_of_the_listing_has_its_own_sentence_and_only_models_point_at_the_picker`
`verified-by: bravebot_cli::chatgpt::the_sign_in_is_said_before_the_listing_is_asked_and_the_listing_is_asked_once`
`verified-by: bravebot_cli::running::auth_login_chatgpt_is_refused_before_a_browser_opens_where_it_cannot_be_done`
`verified-by: bravebot_agent::backend::an_account_failure_is_diagnosed_by_its_step_and_names_the_command_where_it_is_the_remedy`
`verified-by: bravebot_agent::backend::a_refusal_of_the_account_is_not_a_refusal_of_the_body`
`verified-by: bravebot_agent::backend::a_refusal_that_names_the_account_never_falls_back_to_another_model_of_it`
`verified-by: bravebot_agent::backend::a_diagnosis_carries_which_failure_of_the_account_it_was`
`verified-by: bravebot_agent::backend::a_failure_with_no_account_in_it_carries_no_account_value`
`verified-by: bravebot_agent::conversation::the_note_about_a_failed_turn_holds_the_category_and_request_count_and_nothing_else`
`verified-by: bravebot_aichat::lib::each_account_failure_has_a_fixed_name_of_its_own`
`verified-by: bravebot_ui_bridge::failure_tests::a_failure_of_the_account_is_sent_as_its_own_name_beside_its_category`
`verified-by: bravebot_ui_bridge::failure_tests::a_failure_with_no_account_in_it_is_sent_with_no_account_name`
`verified-by: bravebot_ui_bridge::failure_tests::the_failure_sent_holds_only_fixed_names_and_counts`
`verified-by: bravebot_ui_bridge::history::a_turn_on_the_account_with_no_sign_in_reaches_the_window_as_not_signed_in`
`verified-by: bravebot_ui_bridge::history::a_refused_key_in_the_window_is_sent_with_no_account_name`
`verified-by: bravebot_ui_bridge::history::a_turn_on_the_account_with_no_sign_in_is_recorded_in_the_sentence_of_its_step`
`verified-by: bravebot_session::sessions::a_failure_of_the_account_is_said_in_the_sentence_of_its_own_step`
`verified-by: bravebot_session::sessions::a_refusal_naming_the_account_says_usage_limit_or_not_permitted_and_never_a_rejected_request`
`verified-by: bravebot_session::sessions::a_refusal_that_named_the_account_says_its_wait_and_its_status_and_no_count`
`verified-by: bravebot_aichat::client::a_refusal_that_names_the_account_keeps_the_status_and_the_wait_it_came_with`
`verified-by: bravebot_agent::backend::a_refusal_that_names_the_account_keeps_its_status_and_wait_in_the_diagnosis`
`verified-by: bravebot_session::sessions::a_failure_with_no_account_in_it_keeps_its_category_line_its_status_and_its_count`
`verified-by: bravebot_tui::app::a_failure_of_the_account_is_said_in_its_own_sentence_in_the_transcript`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/agent-settings.test.mjs holds the sentence the agent sends to the card's description, two sentences to two cards, and a failure with no sentence to the category's wording; ui/scripts/ux-state.test.mjs folds turn.error through the window's reducer and draws the real row, and ui/scripts/plan-card.test.mjs does the same for a run that failed; make check-ui runs them, and the governs list above holds failure.ts to existing)`
`verified-by: bravebot_aichat::client::a_second_refusal_or_a_replacement_that_cannot_be_made_ends_the_turn_in_the_sign_ins_words`
`verified-by: bravebot_aichat::client::a_refusal_naming_a_listed_code_ends_the_turn_and_replaces_nothing`
`verified-by: bravebot_aichat::client::a_listed_code_is_not_repeated_at_whatever_status_it_comes_with`
`verified-by: bravebot_aichat::client::a_rate_limit_that_names_no_listed_code_is_asked_for_again`
`verified-by: bravebot_aichat::client::a_listed_code_reported_on_the_stream_ends_the_turn_in_the_same_sentence`
`verified-by: bravebot_aichat::client::a_gateways_reply_naming_a_listed_code_is_not_the_account_being_refused`
`verified-by: bravebot_aichat::responses::protocol::the_listed_codes_are_the_seven_the_clause_names_and_the_usage_limit_is_told_apart`
`verified-by: bravebot_aichat::responses::protocol::only_a_refusals_error_code_that_is_a_listed_code_says_something_of_the_account`
`verified-by: bravebot_aichat::responses::protocol::a_listed_code_reported_on_the_stream_is_a_refusal_and_no_other_failure`

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

1. An entry named `chatgpt` that states no endpoint is read for its `models` and for nothing else, and it is
   not a gateway: it names no destination, so the account stays reachable. A model's window is the one its
   own `limit` states, read by the rule a gateway's model follows, under which `context` and `output` both
   have to be stated and the output is read for nothing else. No other field of the entry or of a model is
   read, the entry's `options.contextWindow` included, since a request to the account carries nothing a
   settings file says ([CHATGPT-10](#CHATGPT-10)). A name is trimmed, a name with nothing left is dropped,
   and two names that trim to the same text make one row. The entry is part of the `provider` block, so a
   machine-level `provider` block replaces it, as it replaces the gateways. Where it names
   models, those are what is offered and nothing is asked, as for a gateway: no listing is requested and the
   access token is not renewed, so the roster is the same offline. They are offered where a sign-in is
   stored, since none of the account's rows is offered without one, and a stored sign-in that has ended and
   has not been found out yet is stored, so the first request says it has ended
   ([CHATGPT-8](#CHATGPT-8)). The person wrote that list, so the service refuses at the first request a model
   it names that the account cannot reach, and no listing is consulted to say so.
2. Where it names none, the serving host is asked for its listing, `GET /models` with the access token as a
   bearer and nothing else, and a redirect in answer is a failure ([CHATGPT-2](#CHATGPT-2)). The body is an
   object whose `models` key holds the rows, and a body of another shape is one that cannot be read. A row of
   the listing is offered if its `visibility` is `list` and `supported_in_api` is true, and a model the
   account cannot reach is not offered. The row's `slug` is the model name, `display_name` its label,
   `context_window` its window, `input_modalities` whether it takes images, and `supported_reasoning_levels`
   the effort levels it takes: a row that lists none reads no effort level, and one that does not mention the
   field is not one claiming none. A row in a form the reader does not expect, such as a window that is not a
   whole number, costs that row its place and keeps the rest.
3. Where the listing cannot be read, because the token could not be had, the request failed or its body is not
   the shape above, a compiled list of model names is offered instead, and the picker says the listing could
   not be read, that the list is built in and that a row may name a model the account cannot reach. A listing
   that is read and is empty offers nothing, and the compiled list is not used. A listing refused in a code
   of [CHATGPT-8](#CHATGPT-8)'s closed list is an answer and not one that could not be read: it offers
   nothing and is said in that code's sentence.

The window comes from the entry or the listing as it does for a gateway's, and the assumed low window applies
where neither states one. An output ceiling is not read from the entry, for the reason above, and no model is
left out for what it can call, since neither source says. The compiled list is a constant of the source that
names models only and takes nothing from a reply. Its names are those of the fallback set in opencode's source
(`fallbackModels` in `packages/core/src/plugin/provider/chatgpt.ts`, at commit `ac68d6a929`). Each row takes
the assumed window, which a turn on one of the account's models compacts against where the listing cannot
be read, and no label of its own, and whether it reads an effort level is what a request has already
learned from the service, as for the model an entry names. None of the names is observed against the
service.

An account whose listing is empty, where the entry names no model, is signed in. The command completes,
says the account reaches no model, and the picker has no row for it. Nothing is invented for a tier nobody named.

The roster is asked of the one function both pickers call, so the terminal and the desktop offer the same
rows. It is asked each time a picker opens, and otherwise only for the window of a model in force that is the
account's, so a session on another model sends the stored token nowhere. No row is offered where no sign-in is
stored, and nothing is asked of the service then. A stored sign-in whose token cannot be had, or whose listing
cannot be read, is offered the compiled list, except that the models an entry names need neither the token
nor the listing, only that the file can be read, and offer no row where it cannot be. The desktop picker says
so in a warning beside the other rosters', and the terminal picker says so in a note in the transcript,
written when the picker opens, so before it draws, at rest or mid-turn. Where the compiled list is offered
both say that it is built in and may name a model the account cannot reach. A listing refused with a code of [CHATGPT-8](#CHATGPT-8)'s closed list is said by both in
that clause's sentence, which does not send the person to sign in again. A person who never signed in is
told nothing, and a listing that was read, empty or not, is not a failure.

A listing the service refuses for its token with a 401 is asked for again once with a replacement, as a
turn's request is, and a second refusal ends it and leaves the compiled list, since the token could not be
had. A refusal whose body names a code of [CHATGPT-8](#CHATGPT-8)'s closed list ends it at once and offers
nothing. A model named `chatgpt/<name>` on the command line or in a settings file selects the account, the
request carries the part after the identifier, and an entry with the id keeps it, and none of the account's
models is offered then.

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

`verified-by: bravebot_config::lib::only_the_qualified_name_selects_the_chatgpt_account_and_only_the_rest_of_it_is_sent`
`verified-by: bravebot_config::lib::a_provider_entry_named_chatgpt_keeps_the_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::backend::a_chatgpt_model_selects_the_account_and_a_bare_name_selects_nothing_of_it`
`verified-by: bravebot_agent::backend::a_gateway_entry_named_chatgpt_keeps_its_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::backend::the_account_answers_for_its_own_models_and_for_nothing_a_credential_of_another_service_is_for`
`verified-by: bravebot_aichat::models::only_a_row_the_account_lists_and_takes_in_the_api_is_offered_under_the_accounts_identifier`
`verified-by: bravebot_aichat::models::a_window_the_account_states_is_taken_and_a_missing_or_malformed_one_is_the_assumed_one`
`verified-by: bravebot_aichat::models::an_account_row_reads_an_effort_level_only_where_the_listing_does_not_say_it_takes_none`
`verified-by: bravebot_aichat::models::an_account_row_in_an_unexpected_form_is_dropped_alone_and_the_rest_is_still_offered`
`verified-by: bravebot_aichat::client::the_accounts_listing_is_asked_for_at_the_serving_host_with_the_token_and_nothing_else`
`verified-by: bravebot_aichat::client::a_redirect_in_answer_to_the_accounts_listing_is_a_failure_and_goes_nowhere`
`verified-by: bravebot_aichat::client::a_reply_that_is_not_the_accounts_listing_cannot_be_read_and_an_empty_one_can`
`verified-by: bravebot_agent::chatgpt::a_machine_nobody_signed_in_on_offers_nothing_and_asks_the_service_nothing`
`verified-by: bravebot_agent::chatgpt::an_account_whose_token_cannot_be_had_is_offered_the_compiled_list_and_asks_the_service_nothing`
`verified-by: bravebot_aichat::models::the_compiled_list_is_rows_under_the_accounts_identifier_one_to_a_name`
`verified-by: bravebot_agent::chatgpt::a_signed_in_account_is_offered_what_the_service_lists_under_its_own_identifier`
`verified-by: bravebot_agent::chatgpt::an_unreadable_listing_is_told_apart_from_an_empty_one`
`verified-by: bravebot_agent::chatgpt::a_listing_refused_for_its_token_is_asked_again_once_with_a_replacement`
`verified-by: bravebot_agent::chatgpt::a_listing_refused_for_a_listed_code_is_not_replaced_or_asked_again`
`verified-by: bravebot_aichat::client::the_accounts_listing_refused_for_a_listed_code_says_so_and_any_other_keeps_its_status`
`verified-by: bravebot_agent::chatgpt::a_gateway_entry_named_chatgpt_keeps_the_name_and_the_account_is_not_asked_about`
`verified-by: bravebot_agent::chatgpt::the_models_an_entry_names_are_offered_in_place_of_the_listing_and_need_no_token`
`verified-by: bravebot_agent::chatgpt::the_models_an_entry_names_are_offered_only_where_somebody_is_signed_in`
`verified-by: bravebot_aichat::models::the_models_an_entry_names_are_rows_under_the_accounts_identifier_with_the_window_it_states`
`verified-by: bravebot_config::provider::a_chatgpt_entry_without_an_endpoint_names_the_accounts_models_and_is_not_a_gateway`
`verified-by: bravebot_config::provider::a_chatgpt_entry_with_a_blank_endpoint_still_names_the_accounts_models`
`verified-by: bravebot_config::provider::only_a_chatgpt_entry_that_states_no_endpoint_names_the_accounts_models`
`verified-by: bravebot_config::lib::the_models_a_chatgpt_entry_names_reach_the_configuration_and_the_account_stays_reachable`
`verified-by: bravebot_config::lib::the_models_a_chatgpt_entry_names_are_not_carried_where_a_gateway_holds_the_id`
`verified-by: bravebot_config::lib::a_machine_level_provider_block_replaces_the_models_a_chatgpt_entry_names`
`verified-by: bravebot_config::settings::only_the_layers_a_provider_block_is_read_from_choose_the_accounts_models`
`verified-by: bravebot_tui::app::the_accounts_rows_are_met_in_reading_order_listed_or_compiled_and_nothing_else_adds_one`
`verified-by: bravebot_tui::app::a_caller_on_a_model_that_is_not_the_accounts_never_asks_the_account`
`verified-by: bravebot_tui::app::a_caller_on_one_of_the_accounts_models_asks_the_account_and_takes_the_window_it_lists`
`verified-by: bravebot_tui::app::the_picker_asks_the_account_whatever_model_is_in_force_and_lists_what_it_offers`
`verified-by: bravebot_ui_bridge::models::the_accounts_models_join_the_roster_and_a_failed_listing_adds_the_compiled_list_and_a_warning`
`verified-by: bravebot_cli::chatgpt::each_answer_of_the_listing_has_its_own_sentence_and_only_models_point_at_the_picker`
`verified-by: bravebot_tui::app::the_picker_says_why_a_signed_in_accounts_models_are_missing_and_says_nothing_otherwise`
`verified-by: bravebot_tui::app::only_an_unreadable_or_a_refused_listing_is_a_problem_the_picker_says`
`verified-by: bravebot_ui_bridge::models::a_turn_in_the_window_asks_the_account_only_for_one_of_its_own_models`
`verified-by: bravebot_ui_bridge::models::the_models_list_asks_the_account_whatever_model_is_configured`

<a id="CHATGPT-10"></a>
### CHATGPT-10: a request to the serving host is made in the Responses protocol and carries the token and the session's identifiers

The serving host is `https://api.openai.com/v1`, and it takes the Responses API. The request, the streamed
events, a tool call and its result, usage and cache figures and the effort level are those of
[BACKEND-61](backends.md#BACKEND-61), the clause that adds that protocol for any provider entry (#2994), and
this service adds no protocol of its own. The effort level maps to the service's reasoning field, and no
request to this host is made in another protocol.

The access token is sent as a bearer token in `authorization`. The request carries four more headers.
`session-id`, `thread-id` and `x-client-request-id` each hold the identifier of the session that makes
the request, which this program generated, and the service derives its prompt-cache affinity from
`session-id`. The fourth is `content-encoding: zstd`: the body is the JSON of [BACKEND-61](backends.md#BACKEND-61)
compressed with zstd into one frame, whatever its size, since the service rejects an uncompressed body over
about 1 MiB and decodes zstd. The frame is what is sent: the length the request states and every check on
the body's size are the frame's. A frame over 1 MiB is not sent: the turn is told the body is too large,
spends no token on it, and shortens the conversation as it does where a service refuses the body. A body
over 1 MiB as it is built and under it as a frame is sent. The limit is the one the program this is read
from states for an uncompressed body, and whether it applies to the frame or to what the frame decodes to
has not been observed. This program adds a `content-encoding` to the request of no other service. Compaction of a
conversation for this service is by summary ([compaction.md](compaction.md)), as it is for every service,
and the service offers no native compaction to use instead.

The identifier is a version 4 value generated from the operating system's random source when a session
begins, and it holds nothing about the person, the account, the project or what was asked. A session carries
it in every request it makes: a turn's rounds, a summary, a question asked beside the work, a goal check, a
commit message, a processor, a vetting, an advisor and the calls of a manifest run. Two sessions of one
process carry two. The terminal begins one with its session and another for `/clear` and for a handoff, the
desktop with each session it opens, resumes or forks, and a plain or background session where it is
assembled. A one-shot run begins one with its first request, and the rounds of a goal run and the checks
between them share it, since each is made from a clone of the first round's task. A delegate is a session
of its own. A session resumed from a record begins a new one, since nothing of it is written to a record.
`/branch` moves the running session onto a copy of its conversation and keeps the identifier, because the
copy shares the prefix the service has cached and the original is not asked anything more. A request made
in no session carries an identifier of its own.

The effort level of a session is fixed when the session begins, except for a model that refuses a level,
which the next paragraph states. Until a model refuses it, the account is sent the level in force at the start
in every request of the session that asks for one, whatever level is chosen afterwards, by `/effort`, by a
skill's file or in any other way, because the service keeps the prompt cache of a session only while its
level stays the same. A request that asks for none, as a summary, a goal check and a commit message do,
carries none, and a session that began with none sends none. A delegate begins at the level of the turn that
spawned it, or its definition's. The desktop offers no way to choose a level, so its sessions begin at none.

A model of the account that refuses a body carrying a level and takes the same body without it reads none
([BACKEND-22](backends.md#BACKEND-22)), and that refusal outranks the level a session began with. The model is
sent no level from then on, by every session of the process, and the terminal and a session in lines say so in
that clause's sentence. The level a session began with is not held against the refusal, since a model that
does not take a level would refuse every request of it.

The terminal keeps the level chosen: it is the level of the next session, which `/clear` begins, and of
every model but the account's. A level chosen on a model of the account that is not the one the session
began with, and a model of the account picked while the chosen level is not the session's, are said in a
sentence that names both levels. A skill's file that asks for a level where the account answers is said to
change nothing, in the notice every front end gets for a skill. The info panel names the level sent, and
`/status` names it and says which level was chosen where that is another. The sentences are in the message
catalog, in English and in French.

Nothing else a reply, a model or a settings file says is added to a request to this host: a gateway's
options are carried into its requests, and this service takes none. The sign-in stores no identifier of the
account and no header value is taken from a reply.

**Why.** The service decides the protocol, and a protocol written for one service is a second copy of the
work for the next. A header a settings file could add would be a way to put a value, or another credential,
on a request this clause promises carries only the token, the session's identifiers and the encoding.
Headers built only
from constants and from values this program generated cannot carry a line break or a length a reply chose.
A body the service refuses for its size would fail a long conversation at the point a person has the most
to lose, so the encoding is not left to a setting, and it is applied to every body so that no size is sent
in a form the others are not. The program this is read from compresses every body of this service, with no
size above which it starts, and sends a body to another service as it is unless a person asked for an
encoding. The encoder is `ruzstd`, which is Rust with no build script and no native code, where the `zstd`
crate would compile the C library into each target this program is built for. libzstd decodes the frames
`ruzstd` writes, through its command line tool at 1.5.7, Node 24's `zlib` and Python 3.15's
`compression.zstd`, for bodies from empty to 8 MiB and for ones that do not compress, and no other decoder
has been tried. The encoder writes one level, roughly zstd level 1, with the frame's checksum and no content
size in the header.
The service derives its cache affinity from the identifier, which is why two sessions do not share one and a
session does not change it. A record that held the identifier would tie the record on this machine to the
requests the service logged for the account, so a session resumed from one begins a new identifier, and a
cache the service still holds for the earlier requests is not found ([Known costs](#known-costs)). A level
that changed in the middle of a session would be a request the service holds no cache for, so it is held as
the identifier is, except for a model that refuses a level and would refuse every request of the session.
The sentences say which level is sent, since a level shown as in force that is not the one
sent would mislead a person about what the session costs.

`verified-by: bravebot_aichat::client::an_account_request_carries_the_token_the_three_session_headers_and_the_zstd_encoding_and_nothing_a_gateway_would`
`verified-by: bravebot_aichat::client::a_body_over_the_limit_that_encodes_under_it_is_sent_and_decodes_to_the_bytes_built`
`verified-by: bravebot_aichat::client::a_body_the_service_would_refuse_for_its_size_is_not_sent_and_spends_no_token`
`verified-by: bravebot_aichat::client::an_account_body_is_encoded_whatever_its_size`
`verified-by: bravebot_aichat::client::a_gateways_body_is_plain_json_with_no_content_encoding`
`verified-by: bravebot_aichat::client::a_gateways_refused_key_is_not_reported_as_the_chatgpt_sign_in_failing`
`verified-by: bravebot_agent::backend::a_body_not_sent_for_its_size_is_a_refusal_of_the_body_and_no_failure_of_the_sign_in`
`verified-by: bravebot_agent::chatgpt::a_sessions_identifier_is_random_and_every_handle_on_it_gives_the_same_one`
`verified-by: bravebot_agent::turn::every_round_of_a_turn_on_the_account_carries_the_identifier_of_its_session`
`verified-by: bravebot_agent::turn::turns_of_one_session_carry_one_identifier_and_another_session_carries_its_own`
`verified-by: bravebot_agent::turn::a_turn_given_no_session_is_a_session_of_its_own`
`verified-by: bravebot_agent::turn::a_clone_of_a_task_is_in_the_session_the_task_is_in`
`verified-by: bravebot_agent::turn::a_turn_on_the_account_sends_the_identifier_of_its_session_in_each_of_the_three_headers`
`verified-by: bravebot_agent::turn::a_summary_a_question_and_a_judgement_carry_the_identifier_of_the_session_that_asked`
`verified-by: bravebot_agent::turn::a_summary_made_inside_a_turn_carries_the_identifier_of_its_session`
`verified-by: bravebot_agent::turn::a_commit_message_is_drafted_in_the_session_that_asked_for_it`
`verified-by: bravebot_agent::turn::the_planning_calls_and_the_steps_of_a_manifest_run_carry_the_identifier_of_its_session`
`verified-by: bravebot_agent::turn::the_advisor_a_turn_consults_is_asked_in_the_session_of_the_turn`
`verified-by: bravebot_agent::turn::a_delegate_is_a_session_of_its_own_and_carries_one_identifier_throughout`
`verified-by: bravebot_agent::processor::a_side_request_to_the_account_is_made_in_the_session_the_chat_was_made_for`
`verified-by: bravebot_tui::app::every_turn_of_a_session_is_built_in_it_and_clearing_begins_another`
`verified-by: bravebot_tui::app::branching_a_session_goes_on_with_the_identifier_of_the_conversation_it_copies`
`verified-by: bravebot_ui_bridge::running::two_sessions_open_in_one_process_carry_identifiers_of_their_own`
`verified-by: bravebot_ui_bridge::account_session_tests::every_turn_of_a_desktop_session_is_built_in_it_and_another_session_in_its_own`
`verified-by: bravebot_ui_bridge::account_session_tests::a_manifest_run_is_built_in_the_session_that_started_it`
`verified-by: bravebot_ui_bridge::account_session_tests::a_session_resumed_from_a_record_begins_an_identifier_each_time`
`verified-by: bravebot_cli::plain_retention_tests::every_prompt_of_a_plain_session_is_built_in_it_and_another_session_has_its_own`
`verified-by: bravebot_cli::main::a_one_shot_run_is_a_session_its_clones_share_and_another_run_is_not`
`verified-by: bravebot_cli::goal_run::the_rounds_of_a_goal_run_are_one_session_to_the_account`
`verified-by: bravebot_cli::chatgpt_session::a_run_carries_one_identifier_in_all_three_headers_and_its_delegate_carries_another`
`verified-by: bravebot_cli::chatgpt_session::the_rounds_and_the_checks_of_a_goal_run_carry_one_identifier`
`verified-by: bravebot_cli::chatgpt_session::a_session_in_lines_carries_one_identifier_through_every_prompt`
`verified-by: bravebot_cli::account_sessions::every_task_and_every_backend_the_code_builds_is_in_a_session_or_listed_as_not`
`verified-by: bravebot_cli::account_sessions::a_test_attribute_hides_a_module_of_tests_and_no_task_or_backend_the_code_builds`
`verified-by: bravebot_cli::account_sessions::a_session_named_for_the_task_beside_a_task_is_not_credited_to_it`
`verified-by: bravebot_cli::account_sessions::a_comment_or_a_string_that_names_a_session_does_not_name_it`
`verified-by: bravebot_cli::account_sessions::a_test_attribute_takes_out_its_own_item_and_nothing_after_it`
`verified-by: bravebot_cli::account_sessions::a_session_of_the_account_is_begun_only_where_a_session_begins`
`verified-by: bravebot_aichat::client::a_session_held_to_the_level_it_began_with_sends_that_level_whatever_a_request_asks_for`
`verified-by: bravebot_aichat::client::a_refusal_is_not_put_down_to_a_level_that_was_never_sent`
`verified-by: bravebot_aichat::client::a_model_of_the_account_that_refuses_the_level_is_sent_none_from_then_on`
`verified-by: bravebot_agent::backend::a_request_to_the_account_is_held_to_the_level_the_session_began_with`
`verified-by: bravebot_agent::turn::a_session_begins_at_the_level_it_was_given_and_a_task_given_none_begins_at_its_own`
`verified-by: bravebot_agent::turn::a_delegate_begins_at_the_level_it_was_given`
`verified-by: bravebot_agent::turn::a_skill_asking_for_a_level_changes_nothing_where_the_account_answers`
`verified-by: bravebot_agent::turn::a_model_of_the_account_that_refuses_the_level_is_reported_as_reading_none`
`verified-by: bravebot_cli::chatgpt_session::a_one_shot_run_asks_for_the_level_it_was_started_with_and_a_check_of_its_goal_for_none`
`verified-by: bravebot_cli::chatgpt_session::a_session_in_lines_asks_for_the_level_chosen_in_every_prompt`
`verified-by: bravebot_tui::app::a_chatgpt_session_keeps_the_level_it_began_with_and_says_so_when_another_is_chosen`
`verified-by: bravebot_tui::app::picking_a_chatgpt_model_says_which_level_it_is_sent_where_that_is_not_the_one_chosen`
`verified-by: bravebot_tui::app::the_model_in_force_is_the_accounts_by_the_pick_or_else_by_the_default`
`verified-by: bravebot_tui::app::a_level_the_account_refused_stops_being_sent_and_is_said`
`verified-by: bravebot_tui::status::on_the_account_the_effort_names_the_level_sent_and_the_one_chosen_where_they_differ`
`verified-by: bravebot_tui::app::the_status_of_a_chatgpt_session_names_the_level_it_sends`
`verified-by: bravebot_tui::panel::the_effort_row_of_a_chatgpt_session_is_the_level_it_was_begun_at`
`verified-by: bravebot_agent::chatgpt_effort_in_french::the_sentences_about_a_level_a_chatgpt_session_keeps_are_said_in_the_chosen_catalog`
`verified-by: bravebot_agent::chatgpt::the_account_is_one_provider_at_the_serving_host`

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

A turn asks for the token, and the renewal takes no mode and writes the file whatever the caller is doing.
The test runs the built binary with `--incognito` against a stand-in through the seam of
[CHATGPT-2](#CHATGPT-2), on Linux and macOS.

`verified-by: bravebot_skus::chatgpt::renew::a_renewal_presents_the_stored_refresh_token_and_writes_what_it_is_given`
`verified-by: bravebot_skus::chatgpt::renew::a_write_that_fails_is_reported_and_tried_again_before_the_next_request`
`verified-by: bravebot_cli::chatgpt_account::an_incognito_session_writes_the_renewal_of_its_sign_in`

## What is built

The sign-in is the `chatgpt` module of `bravebot-skus` and the command is `crates/cli/src/chatgpt.rs`.
[LAYER-1](layering.md#LAYER-1)'s row for `bravebot-skus` covers the module, which depends on no crate of this
workspace, so [CHATGPT-5](#CHATGPT-5)'s first condition holds by reading its manifest. The signature of the ID
token is checked with `ring`, the library `rustls` already brings, at the version and with the features it
brings.

The requests a turn sends are made by the Responses client of `bravebot-aichat` given a token source, and
`crates/agent/src/chatgpt.rs` is that source: one renewer for the process, which keeps a renewal the file could
not take for the rest of it, and the note of that, which the turn says once. The same client encodes the body
of a request to the account with `ruzstd` ([CHATGPT-10](#CHATGPT-10)). The identifier and the level a session
carries are an `AccountSession` in the same file, which whatever holds a session begins and hands to each
turn and each side request it makes.

- **Built whole:** [CHATGPT-1](#CHATGPT-1), [CHATGPT-2](#CHATGPT-2), [CHATGPT-3](#CHATGPT-3),
  [CHATGPT-4](#CHATGPT-4), [CHATGPT-5](#CHATGPT-5), [CHATGPT-6](#CHATGPT-6), [CHATGPT-7](#CHATGPT-7),
  [CHATGPT-8](#CHATGPT-8), [CHATGPT-9](#CHATGPT-9), [CHATGPT-10](#CHATGPT-10),
  [CHATGPT-11](#CHATGPT-11), [CHATGPT-12](#CHATGPT-12), [CHATGPT-13](#CHATGPT-13) and
  [CHATGPT-14](#CHATGPT-14).

A sign-in made now is used by a turn whose model is named `chatgpt/<name>`, and the pickers list the models a
`chatgpt` entry names, or where it names none, the models the serving host's listing offers, or where the
listing cannot be read, the compiled names. Once the sign-in
is stored the command asks the listing, as a picker does, and tells the person to pick a model in `/model` or
to name one, that the account reaches no model, or that the listing could not be had.

## Decisions left open

These are a person's to decide, and no test can settle them. The front matter's `status` stays `proposed`
while the first of them is open, although every clause is built.

- **Whether OpenAI's terms allow this.** The sign-in registers a client of its own, named `bravebot`, and
  presents no other program's identifier. Whether ChatGPT's terms allow a third-party program to use a
  subscription this way, and whether OpenAI's registration accepts this program, has to be decided before
  this ships. If it does not, this should not land as written.
- **The compiled list.** [CHATGPT-9](#CHATGPT-9) offers one where the listing cannot be read. Its contents
  are the fallback set of opencode's source, a constant a release changes, and none of its names is observed
  against the service. The alternative is to offer nothing in that case and say to name the models in the
  `chatgpt` entry.

## What the exchange has to record

A sign-in made with a real ChatGPT Plus or Pro account, with the requests it makes observed, has to
establish each of the following before a clause states it as observed. What is recorded is a name, a shape,
a status, a duration or a host. A token, code, state, nonce, challenge, verifier, client or host identifier
or claim value is never recorded, in this repository or in the tracker.

**Observed against the real service**, as a name, a shape, a status or a size taken from a sign-in made with a
real ChatGPT account on this program's own build:

- The authorization endpoint and token endpoint on `auth.openai.com`, with the paths of
  [CHATGPT-2](#CHATGPT-2), complete a sign-in through a loopback listener on a port the operating system
  chooses.
- The authorization server registers a client for the placeholder client and `agent_name_hint`: the callback
  carries a `client_id`, and a refresh presenting it is accepted.
- The scopes and the `resource`, `nonce` and `ext_agent_host_id` parameters are accepted, and the reply's
  `scope` holds `chatgpt.tokens.use.direct`, since the command refuses a reply that lacks it.
- The field names of the code exchange and of the refresh are accepted, both form encoded. A reply to a
  refresh holds a new `access_token` and a new `refresh_token`, and an expiry 3600 seconds out, which is
  `expires_in` or the default the renewal assumes where it is absent. The two are not told apart.
- **A refresh replaces the refresh token.** The client and host identifiers are unchanged. The claim and the
  write-before-use of [CHATGPT-7](#CHATGPT-7) are therefore required and not a precaution.
- The listing, `GET /models` on `https://api.openai.com/v1` with the access token as the only header, is
  answered 200. The body is an object with one key, `models`, holding the rows, and is about half a megabyte
  for nine rows, since a row carries long instruction text the picker does not read. Each row has `slug`,
  `display_name`, `visibility` (`list` or `hide`), `supported_in_api`, `context_window`, `input_modalities`
  (a list of words) and `supported_reasoning_levels` (a list of objects with an `effort` and a
  `description`). Of nine rows, seven are `list`. The efforts go beyond `low`, `medium` and `high`, up to
  `max` and `ultra` on some rows.
- `POST /responses` on the serving host, with the access token as the only header, a body with `stream` `true`
  and `store` `false`, and a listed model, is answered 200 as server-sent events. This holds for a text
  turn, for a turn that calls a function and for a turn that reasons, on the two models tried.
- A text turn's events are `response.created`, `response.in_progress`, `response.output_item.added`,
  `response.content_part.added`, `response.output_text.delta`, `response.metadata`,
  `response.output_text.done`, `response.content_part.done`, `response.output_item.done` and
  `response.completed`. A function call replaces the content events with
  `response.function_call_arguments.delta` and `response.function_call_arguments.done`, and a reasoning turn
  adds `response.reasoning_summary_part.added`, `response.reasoning_summary_text.delta`,
  `response.reasoning_summary_text.done` and `response.reasoning_summary_part.done` before the text.
- **The `output` list of the final `response` is empty.** The items arrive in `response.output_item.done`.
- A function call is an item of type `function_call` with `call_id`, `name` and `arguments`, the arguments a
  string. A `function_call_output` item with the same `call_id` and the result as text answers it, and the
  next turn is accepted with the call item as it arrived, with its `id` left off, and with a reasoning item
  from the same round left in or left out.
- Usage is `input_tokens`, `output_tokens`, `total_tokens`, `input_tokens_details` with `cached_tokens` and
  `cache_write_tokens`, and `output_tokens_details` with `reasoning_tokens`.
- `instructions` is optional, and `max_output_tokens`, `tool_choice`, a strict function schema and
  `reasoning` with an `effort` and a `summary` are accepted.
- A request that is not streamed is refused with 400, a request carrying `temperature` is refused with 400,
  and a model the account cannot use is refused with 400. Each body is an object with one field, `detail`,
  holding a sentence.
- A request without `session-id`, `thread-id` or `x-client-request-id` is accepted.

**Stated by the clauses from opencode's source, to confirm:**

- The key set on `auth.openai.com`, with its path in [CHATGPT-2](#CHATGPT-2).
- That the authorization server accepts a redirect to `http://127.0.0.1:<port>/auth/callback` for any port.
  A port the operating system chose is accepted and no other is confirmed.
- The `id_token` field of the code exchange's reply, which the sign-in requires and verifies.
- The ID token's algorithm, audience, required claims and the nonce match, and the `kid` and key types of the
  key set.
- The ID token's issuer. The sign-in compares it with the constant `https://auth.openai.com`, which is not
  observed from the service: a real token that names another refuses every sign-in.
- That each name of the compiled list ([CHATGPT-9](#CHATGPT-9)) is a model the service takes, and in
  particular the names ending in `-fast`, which are names of the program the list is read from.
- What the three session headers do for prompt-cache affinity, and that there is no native compaction and no
  change of effort in a session.
- That the service decodes a zstd body, which the program this is read from sends for every request to it.
  The frames this program writes differ from the ones that program's `zlib` writes in having a checksum and
  no content size, at a lower level. That the service takes such a frame, the size at which it rejects a
  body, and whether that size is of the frame or of what it decodes to, are not observed.
- The error codes that end the use of a subscription: `subscription_sharing_usage_limit_exceeded`,
  `subscription_sharing_user_not_eligible`, `subscription_sharing_unsupported_capability`,
  `subscription_sharing_route_not_supported`, `subscription_sharing_invalid_user`,
  `chatpass_v2_scope_not_authorized` and `chatpass_v2_invalid_authorization_context`, and where they are
  written: `error.code` in the body of a refusal, as opencode's tests have it for a 429,
  `response.error.code` in a `response.failed` event and `code` in an `error` event.

**Not in any source this spec was written from:**

- What a refused refresh and a refused access token return: the status and the error code. The renewal reads
  `invalid_grant` at a 400 or 401 as ending the sign-in, from RFC 6749 and not from this server.
- Whether a request to the token endpoint is ever answered with a redirect.
- What an account with no subscription reaches.
- Whether an access token minted before a revocation survives it, and whether a sender-constrained form is
  offered.
- What the serving host does with a picture, with a model's own words sent back as `output_text`, with a
  `response.incomplete` or `response.failed` event, and with a refused access token on `/responses`.
- Whether `cached_tokens` is counted inside `input_tokens`: every figure seen was zero.
- Whether the listing is dependable, and what a listing that includes a model the account cannot reach does
  at the first request.
- Whether OpenAI limits the clients registered for one host identifier, and what a second sign-in on a
  machine that has signed out registers.

## What this changes elsewhere

These are the places where another spec states something about this service, each written in that spec.

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
  also gains the case that the effort level of a session is fixed when it starts, except for a model that
  refuses a level ([BACKEND-22](backends.md#BACKEND-22)), and the aichat client gains a zstd body encoding,
  which [BACKEND-61](backends.md#BACKEND-61) states.
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
- A renewal the file could not take is said by the first turn that makes a request after it. A process that
  exits before one does, such as one that renewed for a compaction and then exited, says nothing, and the
  next start finds the sign-in ended. The terminal shows a notice it has already shown in a conversation
  once, so a failure after a write succeeded is not shown there a second time. The sentence names no path,
  system error or cause, so a full disk and a folder that cannot be written get the same advice.
- A person on a machine with no browser forwards the listener's port, which changes at every sign-in, and
  no device-code route is available to this program.
- Where the listing cannot be read the pickers list the compiled names ([CHATGPT-9](#CHATGPT-9)), and a
  person who wants another types `chatgpt/<name>`. The names are another program's and none has been
  observed against the service, so one may name a model the account cannot reach. The service refuses it
  at the first request, and the turn says that the service rejected the request, without naming the model.
- The egress layer reads at most 2 MiB of a reply, and the listing is about half a megabyte for nine rows,
  so an account whose listing passes 2 MiB is read as one that cannot be read and is offered the compiled
  names.
- Where an entry names models ([CHATGPT-9](#CHATGPT-9)), a model named directly that the entry does not name
  has no row, so a turn on it compacts against the assumed window and not the one the listing would give.
  Stating the model in the entry, with its `limit`, gives it one.
- One account is signed in at a time on one machine, as one subscription is imported at a time.
- A person whose gateway entry already has the id `chatgpt` renames it to sign in.
- A reply that names another model than the one asked for is not reported as a substitution for the account,
  since whether the host answers under the name asked or under a dated one is not observed.
- The body is encoded at the one level `ruzstd` writes, which makes frames about 1.4 times the size of
  libzstd's default level on request-shaped JSON (0.40 of the input against 0.27 to 0.29, measured on bodies
  built from this repository's files), and a frame over 1 MiB is not sent. It matches a repeat only within
  128 KiB, so a conversation that holds the same large text twice, far apart, gains nothing from the second
  copy. A long conversation is shortened sooner than it would be by the program this is read from, and the
  limit has not been observed.
- The frame has a checksum and no content size, where the frames Node's `zlib` writes, which the program this
  is read from calls, have a content size and no checksum. A service that needs the size before it reads the
  frame refuses every body. The change that follows is the `zstd` crate, which writes the size, at the cost
  of compiling its C library for each target.
- A picker asks the serving host for the listing each time it opens, and the sign-in command asks it when the
  sign-in is stored, so the access token is sent to the serving host in requests that are not a turn.
- A sign-in that ended while no picker was open is found when the picker opens, which says nothing about it,
  since the renewal that finds it removes the file. The next turn on a `chatgpt/` model says so.
- A session resumed from a record, and a fork begun from one, carry an identifier of their own, so the first
  request after either does not find a cache the service still holds for the earlier session's requests.
- A model of the account that refuses the level ends it for that model for the rest of the process, so a
  session that began at a level is sent none by that model, and so is every session after it. A body
  refused for another reason that does not recur is taken for a refusal of the level when it is taken
  without it.
- A session sends the account two levels. The rounds of its turns, and the planning calls of a manifest run,
  carry the level it began with, and a summary, a question beside the work, a goal check, a commit message,
  a processor, a vetting and an advisor ask for none, so the service applies its own default to them. Whether
  the service keeps the cache of a session across requests that differ in level has not been observed.
- A level chosen in a session on a model of the account applies from the next session, so a person who
  wants another level for the account begins one with `/clear` or by starting again. A session that began
  on a model that reads no level was begun at the level chosen all the same, and the account is sent it.
- A skill's file that asks for a level changes nothing where the account answers, in every front end, and
  the desktop is told so in the notice a skill gets although it offers no way to choose a level.
- The codes that say the account has reached its usage limit or may not be used this way, and the fields of a
  refusal and of a failed event that hold them, are read from another program's source and have not been seen
  from this service. A refusal that says it in another code or another field is handled as its status: a 401
  is replaced once, a 429 or a 5xx is asked for again, and the turn ends with the sentence of that status.
  The two codes that program retries (`subscription_sharing_usage_unavailable` and
  `subscription_sharing_user_unavailable`) are not in the list and are handled the same way.
- The sentences of [CHATGPT-8](#CHATGPT-8) are written in two places: the one-shot run's report and the
  terminal's catalog, which a test holds equal to it. The desktop is sent the catalog's sentence and
  keeps none. The eleven names are listed in `ui/docs/phase-0-rpc-protocol.md`.
- The code that says a refresh token is no longer valid is RFC 6749's and has not been seen from this
  server. A server that ends a sign-in with another code leaves the file in place, and each later turn tries
  the dead refresh token and is told the refresh failed, until the person signs in again.
- A code exchange whose reply holds no ID token ends the sign-in, so a server that stopped sending one, or
  that named another issuer, would refuse every sign-in until a release changes the check. The expiry is
  compared with the clock with no allowance, so a clock running ahead of the server's by more than the
  token's lifetime refuses the sign-in.
- The binary `cargo test -p bravebot-cli` builds, and one `cargo build --all-targets` builds, is a debug
  build with the `test-hosts` feature, so in it `BRAVEBOT_TEST_CHATGPT_STAND_IN` sends the sign-in and its
  tokens where it says. A person who runs that binary from `target/debug` as their own carries the seam.
  `cargo test --workspace` gives the feature to every workspace binary it builds, and `cargo test --release`
  and `cargo build --release --all-targets` stop at the guard that refuses a release build with it.
- Whether a third-party program may use a subscription this way is not this spec's to settle
  ([Decisions left open](#decisions-left-open)). The design assumes a person is allowed to, and an answer of
  no ends it.
