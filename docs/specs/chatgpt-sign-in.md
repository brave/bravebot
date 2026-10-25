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
  - crates/ui-bridge/src/account_words.rs
  - crates/ui-bridge/src/turn.rs
  - crates/agent/src/report.rs
  - crates/agent/src/turn/round_steps.rs
  - crates/session/src/sessions.rs
  - ui/src/renderer/failure.ts
  - ui/src/renderer/apply.ts
  - ui/src/renderer/transcript.ts
  - ui/src/renderer/components/ErrorCard.tsx
  - ui/src/renderer/components/FailureSentence.tsx
  - ui/src/renderer/components/ModelPicker.tsx
  - ui/src/renderer/components/ModelWarnings.tsx
  - ui/src/renderer/components/Transcript.tsx
  - ui/src/shared/protocol.ts
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
  - docs/website/docs/customize/providers/chatgpt.md
  - docs/website/docs/customize/signing-in.md
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
lists what a sign-in made with a real account has observed, what the service's public documents and OpenAI's
documentation state, and what has not been observed. The points where this spec changes another spec are
named in [What this changes elsewhere](#what-this-changes-elsewhere), and each of those specs holds the
change.

The hosts, parameters, field names and headers the clauses state are read from opencode's source
(`packages/core/src/plugin/provider/chatgpt.ts`, at commit `ac68d6a929`), which is another program's code.
Each is a value to confirm with a sign-in unless the observed list of
[What the exchange has to record](#what-the-exchange-has-to-record) includes it. That section records what
documents state. A statement of a document shows nothing about what the service did for this program. The
section also lists what no source states. Whether OpenAI's terms allow a program to use a subscription this
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
`verified-by: bravebot_cli::layering::the_scan_for_the_chatgpt_sign_in_finds_each_way_to_name_it_and_not_a_read_of_the_file`
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
name. A test points the flow at a server of its own through a seam that exists only in a build with the
`test-hosts` feature of `bravebot-skus` and of `bravebot-config`, debug assertions on and a panic that
unwinds, under which the variable `BRAVEBOT_TEST_CHATGPT_STAND_IN` names one server that stands in for the four
destinations. Only the `[dev-dependencies]` of `bravebot-cli`, `bravebot-tui` and `bravebot-ui-bridge` enable
it, so the binaries `cargo test` builds for those crates' tests have it and a release build does not. A build
without the feature holds no code that reads the variable. A build with it that has debug assertions off, or
aborts on a panic, does not compile, by `--all-features` or by a manifest.

Rust has no condition for the release profile, so the refusal is the two settings that profile has and a test
build does not. The release profile has debug assertions off and aborts on a panic, so a release given
`debug-assertions = true` is refused by the abort, and `cargo test --release` is refused by the missing
assertions. An optimised build with assertions on that unwinds is a test build and is not refused, since
nothing the compiler is asked about says how much it optimises. A test compiles the refusal under each
combination of the settings, and reads the root manifest for the release profile's `panic = "abort"`, which
the refusal depends on.

Four more tests read the tree for a way the seam could reach a release, and each reads what is named here and
no more.

- One reads every `Cargo.toml` in the repository, the fuzz package's included, and every `.cargo/config*`
  file, and walks none of `target`, `node_modules`, `.git`, `.worktrees` and `__pycache__`. It fails if the
  feature is named anywhere but its declaration in `bravebot-skus` and `bravebot-config` and the
  `[dev-dependencies]` of the three crates above. It decodes the escapes of a TOML string that write a
  character by its number (`\u002d`) before it looks, so `"test\u002dhosts"` is the feature, because the
  workspace has no TOML parser and the crate of the test depends on none. It fails as
  well if one of these files defines a profile cargo does not have of its own, or spells a way to undo what
  refuses the seam in a release, both as the next test does.
- One reads every text file of the repository, whatever it is called or ends in: a script that ends in
  `.bash`, a `Justfile` and a `Makefile.local` are read as a `Makefile` is. It walks none of the directories
  above or of `dist`, `out`, `build` and `.docusaurus`, which are build output, and leaves out `docs/specs`,
  which says in words what the test looks for, every `Cargo.toml`, which the first test reads, every Rust
  source (`.rs`), which the third reads for who may name a host, and the lock files `Cargo.lock`,
  `package-lock.json`, `pnpm-lock.yaml`, `yarn.lock` and `Podfile.lock`. A file with a NUL byte in it is not
  text and is not read. It reads no command a person types elsewhere. It fails if any file names the feature
  or the variable that names a stand-in at all, in a flag, a list of arguments, a variable or a line of its
  own, in capitals, with `_` for `-`, quoted in pieces, split over lines or written with the escapes of a
  string. It fails if any file asks for every feature (`--all-features`, `all_features`, or
  `all-features = true` in `deny.toml`) in a form the lines of the file do not show, or on a line that is not
  one of the thirteen the test names with their file and how often each occurs: the `cargo clippy` runs in
  `Makefile`, `.github/workflows/ci.yml`, `.githooks/pre-commit`, `contrib/check-lint-increase.py`,
  `agents/pr_worktree.py` and `agents/skills/maintenance/maintenance.py`, which compile and ship nothing, the
  plan `agents/pr_worktree_selftest.py` checks the second of those against, which runs nothing, the
  descriptions of them in `ui/docs/testing.md`, `ui/docs/development.md` and
  `ui/docs/v0.9.0-implementation.md`, and the setting of `deny.toml`, which reads the dependency graph and
  compiles nothing. It fails if any file spells a way to undo the two things a release has that the refusal
  reads: `debug-assertions` in any form (`-C debug-assertions`, a profile setting), `panic=unwind`, or
  `CARGO_PROFILE_RELEASE_` (a variable, or a `--config` of the release profile), however they are quoted,
  split or capitalised. It fails if any file names a profile cargo does not have of its own, by `--profile`,
  by a `--config profile.<name>`, by a `CARGO_PROFILE_<NAME>_` variable or by a table, since one can inherit
  `dev` and be built with `--all-targets`. The profiles cargo has are `dev`, `release`, `test` and `bench`.
  The `--profile` of another tool is named by its file and word: `minimal` in `Makefile` and
  `docs/development/fuzzing.md`, which is `rustup`'s, and `<name>` in `Makefile`, which is the AWS CLI's in a
  message. A test holds the lists in this paragraph to the lists the test reads.
- One reads the source a release build compiles, which is every file under a crate's `src` except test
  modules, the files only a test module declares, and comments, and pins who may name a host and who holds
  the token. `Hosts::at` has no caller. `Hosts::compiled_in` has two, the agent's one renewer and the sign-in
  command, and `Renewer::new` has one, in the agent's renewer. `ResponsesClient::for_account` has one caller,
  in the agent's `backend.rs`, and `list_from_account` has one, in its `chatgpt.rs`. Each sends the access
  token to whatever `Provider` it is given, so the provider each is given is, by the text of the call, the
  one the agent's `account_provider` holds. That function is written four times, twice in `backend.rs` (the
  address of the completion route and the client) and twice in `chatgpt.rs` (its definition and the roster),
  and `account_models_from`, which asks for the listing, has one caller, the roster, which gives it that
  provider. `Provider::chatgpt_account`, which builds the provider whose address is the serving host, has one
  caller, in the agent. The functions that take a `Hosts` are those of the sign-in's own module, by name.
  The access token is asked for with `.access_token()` once in the account's client and once in the agent's
  roster, and the one implementation of that call in the sign-in module is the renewer's.
- One compares the bodies of `Hosts::compiled_in`, `chatgpt_serving_host` and `Provider::chatgpt_account` to
  their text, so an environment read there that is not behind the gate, or a host that is not one of the four
  constants, fails. The name of the variable is a literal in the two gated constants and in no other source,
  build script or command, and the constants are read in the two gated reads and nowhere else.

A second caller of any of these, another reader of the variable, or another function that takes a `Hosts`
fails a test until this clause says why.

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
`verified-by: bravebot_skus::chatgpt::renew::a_redirect_in_answer_to_a_refresh_is_not_followed_and_the_sign_in_is_left_alone`
`verified-by: bravebot_skus::chatgpt::a_first_sign_in_exchanges_the_code_under_the_issued_client_and_stores_the_account`
`verified-by: bravebot_skus::chatgpt::a_key_set_that_cannot_be_had_stops_the_sign_in_and_stores_nothing`
`verified-by: bravebot_cli::test_hosts_seam::a_test_seam_is_enabled_by_dev_dependencies_alone`
`verified-by: bravebot_cli::test_hosts_seam::the_seam_reader_finds_a_seam_wherever_a_build_that_ships_reads_it`
`verified-by: bravebot_cli::test_hosts_seam::a_build_that_aborts_on_a_panic_or_asserts_nothing_refuses_the_seam`
`verified-by: bravebot_cli::test_hosts_seam::no_build_or_release_command_passes_a_seam_or_all_features_but_the_lint_runs_by_name`
`verified-by: bravebot_cli::test_hosts_seam::the_command_reader_sees_a_seam_and_every_feature_however_a_command_spells_them`
`verified-by: bravebot_cli::test_hosts_seam::the_command_reader_sees_what_undoes_the_refusal_and_a_profile_of_its_own`
`verified-by: bravebot_cli::test_hosts_seam::the_command_walk_reads_a_file_whatever_it_is_called_and_leaves_out_what_the_clause_names`
`verified-by: bravebot_cli::test_hosts_seam::the_clause_names_what_the_command_guard_reads_and_leaves_out`
`verified-by: bravebot_cli::test_hosts_seam::the_bodies_that_make_a_host_and_every_reader_of_the_stand_in_variable_and_the_token_are_pinned`
`verified-by: bravebot_cli::test_hosts_seam::only_the_agent_names_the_host_the_account_is_sent_to_and_no_release_calls_the_seam`
`verified-by: bravebot_cli::test_hosts_seam::the_caller_scan_reads_code_and_leaves_out_tests_and_comments`
`verified-by: bravebot_cli::test_hosts_seam::the_call_and_signature_readers_split_where_rust_splits`
`verified-by: bravebot_cli::test_hosts_seam::a_file_only_a_test_module_declares_is_not_a_release_source`
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
  ([CHATGPT-7](#CHATGPT-7)). A refused model request adds three more, that the account has reached its usage
  limit, that it may not be used this way and that it does not offer the model, chosen by comparing the error
  code of the body with a closed list of codes the exchange records and with the one code of a model not
  offered ([CHATGPT-8](#CHATGPT-8)); that comparison is made in the client that sends the request, which is
  not the driver, and the egress crate hands the client the whole body of the refusal under the label of its
  request and reads none of it ([NET-12](network-egress.md#NET-12)). A turn already branches on the
  categories, and none is content anyone authored. Which of these values a failure was is carried to the
  terminal and the desktop beside its category, where it chooses a sentence ([CHATGPT-8](#CHATGPT-8)), and
  the driver adds no comparison of its own for it. The rest of a reply's body, its error text and its claims
  stay in the crate. That is the ordinary case the document's section on the inverse mistake describes, and
  the same footing as the fields of a model listing the code already reads.
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
   [CHATGPT-8](#CHATGPT-8), and whether the file took a renewal, which is the result of a write on this
   machine. The value is chosen where the failure arises: by the step that failed, by the client
   comparing a refusal's code with the closed list, by the client comparing it with the one code of a model
   the account does not offer, or, for a renewal, by the sign-in crate classing the authorization server's
   status (`class_of`, in `bravebot-skus`). A refusal of the model is read from `error.code` apart from the
   closed list, and it is not a refusal of the sign-in. The agent maps the value to a category by
   matching on it and reads no status for that. A refusal that names the account also carries the status of
   its reply and the wait it asked for, as numbers, the wait bounded as any other is. They are carried for
   a front end to display. The one comparison on the status is `AccountFailure::wait`, in `bravebot-aichat`,
   which keeps a wait only for a refusal that came as a 429. No branch of the driver reads either number
   beyond whether there is one, and the rounding of a wait to minutes for the line a person reads. No
   string taken from a reply is part of any of them.
3. The ID token is verified once, at the sign-in, before anything is stored, and no claim of it is
   stored, sent or displayed. A code exchange whose reply holds no `id_token`, or one that is not a
   string, ends the sign-in as a token that did not verify, so no sign-in is stored unverified. A token
   longer than 16 KiB is refused before it is decoded. Each of its three parts is base64url with no padding
   and no bit after the last byte that is not zero, and a part spelled otherwise is refused. Its header is an
   object that names `RS256`, spelled so, and a `kid` that is a string, and nothing that has to be understood:
   `none`, an HMAC and every other algorithm are refused, and so is a header with `crit`. No other field of
   the header is read, so a `jku`, `x5u`, `jwk` or `x5c` names no key. The key is the only key of the set with
   the header's `kid`, and it is an RSA key a signature may be checked with: `kty` is `RSA`, `use`, `alg` and
   `key_ops`, where present, say `sig`, `RS256` and `verify`, the modulus is 2048 to 8192 bits, which are the
   limits of the signature check's own parameters, and the exponent is 65537 or more. That is the least
   FIPS 186-5 allows, and the check's parameters alone take an exponent as small as 3.
   [What the exchange has to record](#what-the-exchange-has-to-record) records the key set as read. A zero
   byte in front of the exponent does not count toward it, and a key written with one in front of the
   exponent or of the modulus is refused by that check, as RFC 7518 has both in the fewest bytes.
   A set that holds two keys under one `kid` has none that is the key. An entry is read for its `kid`,
   `kty`, `use`, `alg`, `key_ops`, `n` and `e` and for no other field, so the `x5c` list that some entries of
   the service's key set carry changes nothing.
   The key set is fetched from the host compiled in for it ([CHATGPT-2](#CHATGPT-2)) with no credential,
   following no redirect, and a reply over 64 KiB is refused. The signature is checked with RS256 against
   that key's modulus and exponent, and is as long as the modulus. The payload is read after that, is an
   object, and its issuer (the constant `https://auth.openai.com`), audience (a string equal to the client
   of the sign-in, or a list holding a string equal to it), expiry (a whole number of seconds later than the
   clock), nonce (the one sent, compared in constant time) and subject (a string that is not empty) are
   compared. No other claim is read. The decoded header and claims are
   cleared when they are dropped. The access token's expiry is read from the `expires_in` field of the
   reply, not from the token, and is compared with the clock. A clock set before 1970 reads as no time, and
   every expiry is later than none, so the clock is read before the listener starts and again where the reply
   arrives, and one that reads before 1970 at either ends the sign-in with nothing stored. Claims such as an
   organisation's name are text somebody else chose.

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
`verified-by: bravebot_skus::chatgpt::id_token::a_key_set_of_several_keys_some_with_an_x5c_list_gives_the_key_the_kid_names_and_no_other`
`verified-by: bravebot_skus::chatgpt::id_token::a_token_the_key_set_signs_for_this_sign_in_verifies_with_one_request`
`verified-by: bravebot_skus::chatgpt::id_token::a_header_that_names_no_key_or_not_rs256_or_is_not_an_object_is_refused_before_the_key_set`
`verified-by: bravebot_skus::chatgpt::id_token::a_key_the_header_names_or_carries_is_never_the_key_the_signature_is_checked_with`
`verified-by: bravebot_skus::chatgpt::id_token::an_expiry_that_is_missing_not_a_whole_number_or_not_after_now_is_refused`
`verified-by: bravebot_skus::chatgpt::id_token::an_audience_that_is_not_the_client_or_a_list_holding_it_is_refused`
`verified-by: bravebot_skus::chatgpt::id_token::an_issuer_a_subject_and_a_nonce_that_are_not_strings_or_not_the_expected_ones_are_refused`
`verified-by: bravebot_skus::chatgpt::id_token::a_token_spelled_with_padding_the_standard_alphabet_or_stray_bits_is_refused_in_every_part`
`verified-by: bravebot_skus::chatgpt::id_token::a_payload_that_is_signed_and_is_not_an_object_of_claims_is_refused`
`verified-by: bravebot_skus::chatgpt::id_token::a_token_keyed_with_the_public_key_as_an_hmac_secret_is_refused`
`verified-by: bravebot_skus::chatgpt::id_token::a_signature_of_another_length_than_the_modulus_is_refused`
`verified-by: bravebot_skus::chatgpt::id_token::a_key_of_fewer_than_2048_bits_or_with_a_small_exponent_is_refused_whatever_its_signature_is`
`verified-by: bravebot_skus::chatgpt::id_token::a_decoded_part_is_overwritten_where_it_lies`
`verified-by: bravebot_skus::chatgpt::id_token::an_exponent_of_65537_or_more_is_used_and_a_smaller_one_is_not_whatever_its_leading_zeros`
`verified-by: bravebot_skus::chatgpt::id_token::the_decoded_header_and_payload_are_held_in_buffers_that_are_cleared_in_each_way_a_check_ends`
`verified-by: bravebot_skus::chatgpt::encoding::base64url_reads_back_what_it_writes_and_nothing_else`
`verified-by: bravebot_skus::chatgpt::encoding::a_clock_before_1970_reads_as_no_time_and_the_epoch_and_after_as_the_seconds_since`
`verified-by: bravebot_skus::chatgpt::a_clock_set_before_1970_ends_the_sign_in_before_anything_is_opened_or_asked`
`verified-by: bravebot_skus::chatgpt::a_clock_that_cannot_be_read_where_the_reply_arrives_ends_the_sign_in_with_nothing_stored`

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
  minted before a revocation survives it is a fact the exchange records, and none has: one minted before a
  refresh was seen to work after it, but a revocation was not made. The record says an access token already
  issued may run to its own expiry, and `doctor` says so with the words that this has not been observed, not
  that nothing survives.
- **Neither is bound to its presenter.** Nothing asks the authorization server for a sender-constrained
  form, so the record says bearer, with the answer `NotAttempted`, until the exchange records whether one
  is offered. The discovery document advertises none
  ([What the exchange has to record](#what-the-exchange-has-to-record)), which does not show that a request
  for one is refused, so the answer stays `NotAttempted`. A refresh sent with a garbage `DPoP` proof was
  refused like one sent without it, which shows no sender-constrained form either.

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
expiry is the instant of the reply plus its `expires_in`, or plus 3600 seconds where the reply has none. The
instant is read from the system clock, and a clock set before 1970 reads as 0 here (Known costs).
Otherwise the refresh token is presented to the token endpoint, with the stored client and the `resource`
of [CHATGPT-3](#CHATGPT-3), the reply is written to the file, and only then is the new access token used. A
refresh reply that grants no refresh token keeps the one presented. No request is sent with a token past its
expiry. A token the serving host refuses before its expiry,
whether the clock here runs behind or the token was revoked early, is replaced in the same way once, and the
request is sent once more, unless the refusal carries a code of [CHATGPT-8](#CHATGPT-8)'s closed list or the
code of a model the account does not offer: those say the account has reached its usage limit, may not be
used this way or does not offer the model, and a new token does not change that. The code is the `error.code` of
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
the one this server sent, as `{"error":"invalid_grant"}` in a 400, for a refresh token it does not know. The
code is read from `error` as a string, so a body whose `error` is an object, as the serving host's refusals
have it, is not that code. A refusal with any other code or status is a failure of the request and ends
nothing. The file is removed, and the turn says the sign-in has ended, whether or not the file could be
removed. The file was read with the claim held, so it holds the refresh token that was refused or one this
process replaced and so killed. Any other failure of the refresh, a connection that did not complete or
a reply that says the service is busy or unavailable, leaves the file as it is and says which of the
backends spec's failure categories it was. A write of the reply that fails is reported to the person, once,
as a notice of the turn, in a sentence that names no path and quotes nothing the file or the system said. The
notice is given after the request that renewed it, and when the turn ends if it has not been given, so a
renewal made by a call that is not a round's request, such as the goal check or a processor, is reported in
the turn it was made in. A renewal made outside a turn, for a picker's listing, the listing a start asks
for before it asks which model to use, the sign-in command or a compaction at rest, is reported by the
next turn that makes a request. The new tokens are used from memory
for the rest of the process, and the process tries the write again before each later request, under the claim,
and only if the file still holds the token this process replaced. A retry that fails again is not reported
again, and one that succeeds clears the report, so a failure after it is reported. Where the file holds
another token, or none, the tokens in memory are dropped.

The renewer keeps that the sign-in ended for the rest of the process. Every later ask of it for a token,
whether a turn's request, a summary, a goal check or the listing a picker makes, finds no file and says the
sign-in has ended, and asks the token endpoint nothing, since a missing file does not say whether a sign-in
was never made or ended. The end is read only where the file holds nothing. A file that holds a sign-in
again is used as any stored sign-in is and the end is forgotten, so a sign-in removed after that is not
stored. `bravebot auth login chatgpt` is the only place a sign-in is made. It runs before any session and
asks the renewer for a token only after it has stored one, so no process holds an end and then signs in
itself, and a sign-in made in another process is found by the file. A process that did not find the end,
such as one that starts after it, finds no file and says no sign-in is stored.

**Why.** An authorization server may replace the refresh token each time it is used, and then the copy a
second process read before the first's reply is dead. A session, a background run and a one-shot run on one
machine present it together as a matter of course, and the one that loses ends the sign-in for all of them.
The claim is held across a network round trip, so it has an age longer than that trip's deadline: the Leo
claim is judged abandoned after seconds because its hold is one local read and write. The reply is written
first because a process that stops between receiving it and writing it has spent a refresh token it cannot
present again. This server replaces the refresh token on use, so the claim and the early write are
required here. A server that did not replace it would lose nothing to them. Removing the file
on an ended sign-in leaves `auth status` and the picker saying what is true, and stops every later turn
from presenting a token that cannot work. The renewal that finds the end is often not a turn's: a one-shot
run asks for the account's listing to learn its model's window before the turn is sent, and a picker asks
for it when it opens. The file is gone when the turn asks, so without the renewer's memory of the end the
turn would say that no sign-in was made, which is not what happened and sends a person to sign in as if
they never had.

A turn asks the renewal before every request (`Renewer::access_token`) and after a refusal of the token
(`Renewer::replace_refused`), which gives a token other than the refused one and never the refused one back.
Only a refusal with the status 401 is a refusal of the token, and one whose body names a code of the closed
list, or the code of a model not offered, ends the turn without a replacement.

`verified-by: bravebot_skus::chatgpt::renew::a_token_with_more_than_a_minute_left_is_used_and_nothing_is_asked`
`verified-by: bravebot_skus::chatgpt::renew::a_token_within_a_minute_of_its_expiry_or_past_it_is_replaced`
`verified-by: bravebot_skus::chatgpt::renew::a_renewal_under_a_clock_before_1970_stores_an_expiry_the_next_start_renews_again`
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
`verified-by: bravebot_skus::chatgpt::renew::a_renewer_that_found_the_sign_in_end_says_so_to_every_later_ask`
`verified-by: bravebot_skus::chatgpt::renew::a_sign_in_stored_after_the_end_is_used_and_the_end_is_forgotten`
`verified-by: bravebot_skus::chatgpt::a_sign_in_made_after_a_renewer_found_the_old_one_end_is_the_one_it_uses`
`verified-by: bravebot_cli::chatgpt_account::a_one_shot_run_whose_sign_in_ended_says_it_ended_and_asks_the_token_endpoint_once`
`verified-by: bravebot_cli::chatgpt_account::a_session_in_lines_says_the_sign_in_ended_to_every_turn_after_it_ended`
`verified-by: bravebot_skus::chatgpt::renew::each_failure_is_named_by_its_step_and_class_alone`
`verified-by: bravebot_skus::chatgpt::exchange::a_refresh_presents_the_refresh_token_under_the_client_and_the_resource`
`verified-by: bravebot_skus::chatgpt::exchange::only_the_recorded_code_at_a_refusing_status_ends_the_sign_in`
`verified-by: bravebot_skus::chatgpt::exchange::a_reply_without_a_refresh_token_keeps_the_one_presented`
`verified-by: bravebot_skus::chatgpt::exchange::the_exchange_refuses_a_reply_that_grants_no_refresh_token`
`verified-by: bravebot_aichat::client::an_account_asks_for_its_token_before_every_request_and_sends_the_one_it_is_given`
`verified-by: bravebot_aichat::client::a_token_the_host_refuses_is_replaced_once_and_the_request_is_sent_again_with_the_new_one`
`verified-by: bravebot_aichat::client::a_second_refusal_or_a_replacement_that_cannot_be_made_ends_the_turn_as_the_sign_in_failing`
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
request, whether its error code is one of a closed list or is the code of a model not offered. None holds
text taken from a reply, a request, a
callback or a settings file. A request to either host that did not succeed reports one
of the backends spec's failure categories: refused, rate-limited, unavailable, incomplete or a transport
failure. The rest are these, each saying which it is:

- **Signing in again is the remedy,** and the sentence names `bravebot auth login chatgpt`: not signed in,
  the sign-in has ended, the service refused the token. Not signed in is said where no sign-in is stored and
  this process has not found one end, and the sign-in has ended is said to every ask for a token that follows
  the renewal that found it end ([CHATGPT-7](#CHATGPT-7)). A renewal that failed in any other way, refused,
  rate-limited, unavailable, incomplete or not completed, names no command in the terminal or the window:
  the file is left as it is and the next request asks again, so signing in again is not offered as the
  remedy for a failure that may pass. Whether signing in again would mend a renewal the server refuses in
  a code this program does not read as the end of the sign-in is not observed (see
  [Known costs](#known-costs)).
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
  authorized) have one sentence that says the account may not be used this way. It names no cause, since it
  is chosen from the code alone, and does not name signing in again as the remedy. Both are a refusal among the
  categories, which no fallback model is used for: the account answers for every model it serves, so the
  fallback would be refused the same way. Of the seven, only `subscription_sharing_unsupported_capability`
  has been seen, as the `error.code` of a 400 for the tool `image_generation`, which this program does not
  send, and its message named the tool ([What the exchange has to record](#what-the-exchange-has-to-record)).
  The sentence's advice can be wrong for that code ([Known costs](#known-costs)).
- **The model is not one the account offers.** A request is refused with the `error.code` `model_not_found`
  in the body of the refusal. The service gave it as a 400 and no other status was seen, so the code is read
  at any status, and a 401 that carried it would replace no token. The sentence says that OpenAI does not
  offer that model to the account and that another can be chosen. It names no model and
  quotes nothing of the body, which names the model in two fields, and it does not name signing in again as
  the remedy, since the sign-in was not what was refused. The request is not repeated, no token is replaced
  and the level is not dropped and the request sent again, since none of these changes what the code says.
  It is a refusal among the categories, which no fallback model is used for, and carries no status or wait:
  it is the sentence alone in each place a failure is worded. Only a body whose `error.code` is the whole of
  that code says it, so a refusal that holds only a `detail`, or holds `error` as a string, is its status
  alone. The code is read in a refusal's reply and not in a `response.failed` event or an `error` event,
  where it has not been seen. A gateway's reply naming the code is that service's own refusal and keeps its
  status.
- **The command stopped,** and says so: the listener could not start, the browser returned no code or no
  usable client, the reply held no ID token or one that did not verify, the key set could not be fetched (in
  the category of that request), the time bound passed, the clock is set before 1970
  ([CHATGPT-5](#CHATGPT-5)), a `provider` entry named `chatgpt` is a
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

Each sentence is a message of the catalog, in English and in French, chosen from the value alone in
`bravebot-agent`. `bravebot-aichat` has no catalog, so it names each condition and holds no sentence: what it
prints for a failure of the account is that name. The one-shot run reports the sentence as the error of the
failed turn, a goal run and a manifest run report it as the reason they stopped, and the terminal says it
as the line for a failed turn, for a failed goal check and for a manifest run, in the language the person
chose. The terminal's `/compact`, the line a turn narrates for a summary it could not make and the line it
narrates for a move to the fallback model ([BACKEND-53](backends.md#BACKEND-53)) say it as well, where a
failure of any other service says the name of its category. A turn hands those two notices to its front end
as the closed value ([`Why`](../../crates/agent/src/report.rs)) and not as a sentence, so the terminal
narrates the terminal's sentence and the window and an editor are told theirs, below. The record of a
session keeps the terminal's line in either front end. It is the whole reason, so no
count of tries follows it, since what it says to do ends it. A refusal that came in a reply says the wait
it asked for in its sentence, in minutes rounded up, in each of them. Its status follows the sentence in the
terminal's line for a failed turn, a failed goal check and a manifest run, in the one-shot run's report, in
a goal or manifest run's reason and in the record of a session: "OpenAI says this ChatGPT account has
reached its usage limit, and the service asked for a wait of about 10 minutes (HTTP 429)". The terminal's
`/compact`, the line a turn narrates for a summary it could not make, the line it narrates for a move to
the fallback model, the window's sentence and an editor's error say the sentence alone, with no status or
count after it, since each says a closed value the agent chose and nothing from a reply: "OpenAI says this
ChatGPT account has reached its usage limit, and the service asked for a wait of about 10 minutes". A
refusal reported in an event of the stream carries neither a wait nor a status and is the sentence alone, as
does every other failure of the account. A failure that carries no value is worded from its category, with
its status and count.

The terminal's sentences are lower case, two sentences where a remedy follows, and write the command
between backticks, none of which a window can draw as it is, so the desktop is sent sentences of its own,
which are catalog messages in `bravebot-ui-bridge`. Each is one sentence in sentence case that says what
happened and what to do, chosen from the value alone. The sentence that sends a person to sign in tells them
to run the command in a terminal, since no window starts the sign-in, and names the command the terminal
names ([CHATGPT-1](#CHATGPT-1)). The sentence is sent as `accountMessage` beside the name, and the words of
it that are the command are sent as `accountCommand`, so the window draws them as code without reading the
sentence for them. A refusal's status is in `status`, and the window's sentence for a refusal that asked
for a wait at a 429 says it, for either refusal and in the same minutes as the terminal does: the wait is the
service's, and which refusal it came with does not change that it is when to ask again. The rule is the
terminal's, the one-shot run's and the window's alike. `accountMessage` is the description of one card in
`ui/src/renderer/failure.ts`, drawn by the error card and by the card of a run that stopped, with the
command in a code element and everything else as text. A failure with no sentence is drawn from its
category. An editor hosting a session is told the same sentence ([ACP-8](acp.md#ACP-8)).

The window is told a summary the account could not make as two narrations of its own, one sentence each:
that the turn goes on with the messages it has, and then the failure's sentence with its command beside it
in `command`, which the narration row draws as code. A move to the fallback model after the account failed
is one narration naming both models and the cause, which names no command. An editor is told the same
lines as text. A failure of any other service is narrated in the terminal's words in all three, so the
window's narration row holds the account's sentences in sentence case beside the terminal's own lines for
any other service, and the window rewords neither.

No code, `state`, `nonce`, verifier, challenge, token, claim, client or host identifier, reply body, header
value or callback query reaches a trace, a log, an error message, a session record, a transcript, an exported
conversation, a result or stream a script reads, the connection of a front end, `doctor`, `auth status`, the
terminal or the environment of a program a turn starts: a hook, a program `run` starts, an MCP server or a
language server. The one exception is one copy of the authorization URL, which carries the `state`, the
`nonce`, the challenge and the identifiers. [CHATGPT-13](#CHATGPT-13) writes it to the terminal of the sign-in
once and it goes nowhere else: no other output holds it, and that terminal holds it once.

A test holds every value the run handled against each of those outputs, as the trail's test does for the
credentials the agent holds. The values are the code, the `state`, the `nonce`, the challenge, the verifier,
both pairs of tokens, the ID token and each of its three dot-separated parts, the client and host identifiers,
the identifier the process made for its session, and the text of each refusal: the stand-in repeats every
value it has seen in the body of every refusal it makes. The authorization URL is held as a value too. The
terminal of the sign-in is held with one copy of it taken out, and fails if it holds none or more than one,
and every other output and every file is held to it whole.

The scan looks for each value as the run wrote it and percent-encoded with upper-case hexadecimal digits,
which is how an address and a form body write one that is not URL-safe, and for the authorization URL as it
was written. It does not find a value written percent-encoded with lower-case digits, in base64 or
hexadecimal, folded to another case, split over two lines of a file or over two rows of the screen, or cut
short, partly redacted or otherwise changed. An output that carries a value in one of those forms passes, so a
hold is a hold against the forms this program's own writers use and not against every way a value could be
written. Each value is required to be longer than eight bytes and not part of another, so that a hit names the
value it found. The ID token and its three parts are the exception, since they are text of the whole by
design.

Each output is required to be there, and to hold something unless nothing writes to it in the run: the
standard error of the bridge and of the ACP adapter, the standard output of the local host, and the standard
output and standard error of the background host, which these runs leave empty, are held when they hold
anything and accepted when they hold nothing, and the two files the background host's output is sent to are
required to exist. A run that wrote none of the other outputs does not pass. Each test below makes the run
itself and holds what its own writers produce from it:

- `bravebot_cli::chatgpt_account::the_authorization_url_is_let_through_once_in_the_sign_in_terminal_and_in_no_other_output`
  holds outputs made by hand against the values of a run. The terminal of the sign-in with the URL once
  passes. It fails with the URL twice or not at all, and with the `state` or a part of the URL besides the one
  copy. Any other output fails with a copy of the URL, in a trace or alone, and with the `state` or the
  `nonce` alone.
- `bravebot_cli::chatgpt_account::the_scanner_finds_a_value_percent_encoded_refuses_an_empty_output_and_refuses_overlapping_values`
  holds values made by hand. It finds one as written and percent-encoded, fails an output with nothing in it,
  and fails a set of values in which two are equal, one is inside another, one is too short or one is empty.
- `bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_an_output_of_a_run_that_signs_in_renews_and_fails`
  runs the built binary against the stand-in through the seam of [CHATGPT-2](#CHATGPT-2): the sign-in in a
  terminal, `auth status` and `doctor` while signed in and once the sign-in has ended, and these print-mode
  turns: one answered as text, one as `--json` and one as `--json-stream`; one in which the model calls `run`
  for a program that writes its environment to a file; one for a workspace that declares an MCP server that
  writes its environment to a file and to its standard error; three refused in a code of the closed list, as
  text, as JSON and as a stream, at the status that otherwise has the token replaced; three refused as a
  model the account does not offer, as text, as JSON and as a stream; one whose refresh is
  refused as no longer valid; and two after the sign-in has ended, as JSON and as a stream. Every command is
  run with `--log-level debug` and each turn with `--trace`, which is an option of a task and of no other
  command. A hook of the home starts a program at each of the two moments of a turn, and the test requires
  that each turn started two and holds the environment they wrote. It holds the sign-in's terminal, each
  command's standard output and standard error, which is where the trace, the JSON result and the stream are,
  the debug log of the sign-in and of each turn, taken as each run ends because a state directory keeps the
  newest ten, every file left in the home, which holds the environments of `run` and of the server, and the
  session record and trail of each answered turn. It requires one log for the sign-in and one for each turn
  and none for `auth status` and `doctor`, which ask the stand-in nothing, one record and one trail for each
  answered turn, and that neither the refusals in a listed code nor the refusals of the model asked for a
  new token. A print-mode turn that fails
  records nothing, so the record of a failed turn is held where one is written.
- `bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_files_of_one_that_stops`
  runs the sign-in in a terminal three times, against a stand-in that refuses the code, issues an ID token
  that names another sign-in's nonce, and cannot serve the key set. It holds the terminal of each, less one
  copy of the authorization URL, and every file each left. Each leaves none, since a sign-in that stops
  stores nothing and writes no log, and the test fails where one leaves a file so that the file is searched.
- `bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_of_the_line_interface`
  runs `bravebot --log-level debug --plain` in a terminal. The model's first reply asks for a language server
  that a person declared, which the person allows, and the server writes the environment it was given to a
  file. Four prompts then fail in four ways. It holds the terminal and every file left, and requires a log for
  the sign-in and one for the session. A session in lines keeps no record of its turns outside a background
  host.
- `bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_record_of_a_background_session`
  runs the background host, `bravebot --log-level debug __bg-host`, with a first prompt in its job directory,
  attaches to its socket with the token the host keeps beside it, and types four more prompts that fail in
  four ways. It holds what the attached terminal is shown, the files the host's standard output and standard
  error were sent to and every file left, requires that the host kept one session record, of the one turn that
  was answered, and requires a log for the sign-in and one for the host. It runs on Linux and macOS.
- `bravebot_tui::chatgpt_failure_tests::what_the_terminal_client_keeps_and_draws_of_a_failed_turn_holds_none_of_the_values_of_the_run`
  signs in and makes four turns that fail, as a renewal while the token endpoint is busy, a refusal that is
  not of the token, a refusal in a code of the closed list, and a sign-in that has ended, and gives each
  failure to `fold_outcome` and `record_turn` as a finished turn is given. It holds the transcript, its
  markdown export, the file `/export` writes after each turn, the screen as cells and as the bytes written to
  a terminal, the turns the record keeps with each reason, the note the picker says for a listing that cannot
  be read and for one refused in a code of the closed list, the error message of each turn, its debug form,
  how it ended, what it reported and the conversation, every file the session wrote but the sign-in file, and
  the terminal of the process that made the run. It requires that the record's reason is the failure the
  transcript shows and that no two of the four turns failed alike. The run is made in a child of the test
  binary, since the account's hosts and the state directory are read from the environment, this crate forbids
  `unsafe`, and `fold_outcome` is private.
- `bravebot_ui_bridge::chatgpt_secrets::what_the_bridge_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
  signs in in a child of the test binary and then drives `bravebot-rpc`, the binary the desktop starts, over
  its pipe through the same four failures and two `models.list` requests, one for a listing that cannot be
  read and one for a listing refused in a code of the closed list. It holds the `turn.error` event of each
  turn, the answer to each `models.list` with its warnings, every line the binary wrote to its pipe, its
  standard error, and the record and trail it wrote. It requires that no two turns failed alike and that the
  record keeps the four.
- `bravebot_ui_bridge::chatgpt_secrets::what_the_acp_adapter_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
  drives `bravebot-acp` over its pipe as an editor does, through `initialize`, `session/new` and
  `session/prompt`, with the same four failures, and answers its request to trust the directory with
  `allow-once`. It holds what the editor is told of each turn that fails, every line the adapter wrote to its
  pipe, its standard error, and the record and trail it wrote. It requires that the record keeps the four and
  that the four errors the editor is told are four distinct messages, since the adapter says the account's
  sentence for each failure of the account ([ACP-8](acp.md#ACP-8)).
- `bravebot_ui_bridge::chatgpt_secrets::what_the_local_host_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
  starts `bravebot-host`, which keeps the bridge's sessions while the program that drives it comes and goes,
  with this test binary run again as that program, which copies the channel the host made for it to a socket
  the test listens on. The test speaks the bridge's protocol through it, with the same four failures. It holds
  every line the client is sent, the host's standard output and standard error, and every file left under the
  sessions root and the home, and requires one record, which keeps the four turns each with a reason of its
  own, and one trail.

Every output the clause names is held by one of these, except what the renderer in `ui/` draws from the events
the bridge sends and the debug log of the full-screen terminal client. The renderer is not a crate, so the
events are what is held, and what the window makes of the `account` name and the sentence in one is pinned by
the scripts the `by-construction` entry of this clause names. The test of the full-screen client is made in a
child of its test binary that sets no log level, and the client's crate has no dependency on the crate that
writes the log, so the debug log of that client is not made to be held, and the files its session wrote are.

Two more things are not reached. The loop in `run_turn_animated` that takes what a turn's reporter sends into
the session needs a terminal, and no test runs it. The test of the terminal client makes its turns with the
client's own `RemoteReporter`, requires that the prompt and the request were reported, holds every `ToMain`
message the reporter sent, and takes each into the session with the call that loop makes for it, so the
reporter and its messages are driven and the loop's own match is not. And the bridge, the ACP adapter and the
host start no debug log, since only `crates/cli/src/main.rs` configures one, so no log of theirs exists to be
held: the files their sessions leave are.

The command's tests run on Linux and macOS, where `script` gives the command a terminal, and not on Windows.

**Why.** A message that quoted an authorization server's error would carry a value it chose to the person and
to anything that saves the message, and a log of a refresh would hold a token for as long as the log.
Choosing from a closed set also keeps [CHATGPT-5](#CHATGPT-5) true: what the agent branches on is a value
this crate computed from a step and a status class, or one of the values the client chose by comparing a
refusal's code with a closed list or with the code of a model not offered. A model the account does not
offer is told apart from a token the host refused and from a body the service would not take, because each
has a remedy of its own: signing in again does not change a model, and a shorter conversation does not
either. The categories are the ones the other services' failures already use, so
a rate limit by status here is waited out and reported as a rate limit is elsewhere, and a remedy that
signing in again would not supply is not offered. A usage limit named by a code is not waited out, since
nothing a second request or a wait of this program's does changes it.

`verified-by: bravebot_cli::chatgpt_account::the_authorization_url_is_let_through_once_in_the_sign_in_terminal_and_in_no_other_output`
`verified-by: bravebot_cli::chatgpt_account::the_scanner_finds_a_value_percent_encoded_refuses_an_empty_output_and_refuses_overlapping_values`
`verified-by: bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_an_output_of_a_run_that_signs_in_renews_and_fails`
`verified-by: bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_files_of_one_that_stops`
`verified-by: bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_of_the_line_interface`
`verified-by: bravebot_cli::chatgpt_account::no_value_of_a_chatgpt_sign_in_reaches_the_terminal_or_the_record_of_a_background_session`
`verified-by: bravebot_tui::chatgpt_failure_tests::what_the_terminal_client_keeps_and_draws_of_a_failed_turn_holds_none_of_the_values_of_the_run`
`verified-by: bravebot_ui_bridge::chatgpt_secrets::what_the_bridge_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
`verified-by: bravebot_ui_bridge::chatgpt_secrets::what_the_acp_adapter_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
`verified-by: bravebot_ui_bridge::chatgpt_secrets::what_the_local_host_sends_and_keeps_of_a_failed_turn_holds_none_of_the_values_of_the_run`
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
`verified-by: bravebot_cli::chatgpt::each_answer_of_the_listing_has_its_own_sentence_and_only_models_send_the_person_to_choose`
`verified-by: bravebot_cli::chatgpt::every_command_the_sign_in_ends_by_naming_is_one_that_works_as_typed`
`verified-by: bravebot_cli::chatgpt::the_sign_in_promises_the_question_only_where_a_start_would_ask_it`
`verified-by: bravebot_cli::chatgpt_start::the_sign_in_command_promises_the_question_only_to_a_person_the_next_start_asks`
`verified-by: bravebot_cli::chatgpt::the_sign_in_is_said_before_the_listing_is_asked_and_the_listing_is_asked_once`
`verified-by: bravebot_cli::running::auth_login_chatgpt_is_refused_before_a_browser_opens_where_it_cannot_be_done`
`verified-by: bravebot_agent::backend::an_account_failure_is_diagnosed_by_its_step_and_names_the_command_where_it_is_the_remedy`
`verified-by: bravebot_agent::backend::a_refusal_of_the_account_is_not_a_refusal_of_the_body`
`verified-by: bravebot_agent::backend::a_refusal_that_names_the_account_never_falls_back_to_another_model_of_it`
`verified-by: bravebot_agent::backend::a_diagnosis_carries_which_failure_of_the_account_it_was`
`verified-by: bravebot_agent::backend::a_failure_with_no_account_in_it_carries_no_account_value`
`verified-by: bravebot_agent::conversation::the_note_about_a_failed_turn_holds_the_category_and_request_count_and_nothing_else`
`verified-by: bravebot_aichat::lib::each_account_failure_has_a_fixed_name_of_its_own`
`verified-by: bravebot_aichat::lib::an_account_failure_is_shown_as_its_name_and_never_as_a_sentence`
`verified-by: bravebot_agent::chatgpt::every_failure_of_the_account_has_its_own_sentence_and_names_the_command_only_as_a_remedy`
`verified-by: bravebot_agent::chatgpt::a_refusal_that_asked_for_a_wait_says_it_in_minutes_and_one_that_did_not_says_none`
`verified-by: bravebot_agent::chatgpt::a_turn_a_goal_check_a_compaction_and_an_advice_print_the_accounts_sentence`
`verified-by: bravebot_agent::chatgpt_failures_in_french::each_failure_of_the_account_is_said_in_the_chosen_catalog_and_names_the_same_command`
`verified-by: bravebot_cli::running::a_turn_that_failed_on_the_chatgpt_account_is_reported_in_the_language_chosen`
`verified-by: bravebot_cli::running::a_goal_run_whose_judge_is_the_chatgpt_account_with_no_sign_in_says_the_sentence`
`verified-by: bravebot_cli::running::a_manifest_run_whose_planner_is_the_chatgpt_account_with_no_sign_in_says_the_sentence`
`verified-by: bravebot_cli::running::a_refusal_that_asked_for_a_wait_is_reported_by_the_one_shot_run_with_the_wait_and_the_status`
`verified-by: bravebot_cli::chatgpt_account::a_one_shot_run_whose_sign_in_ended_says_it_ended_and_asks_the_token_endpoint_once`
`verified-by: bravebot_cli::chatgpt_account::a_session_in_lines_says_the_sign_in_ended_to_every_turn_after_it_ended`
`verified-by: bravebot_tui::app::a_goal_check_that_failed_on_the_chatgpt_account_says_its_own_sentence`
`verified-by: bravebot_tui::app::a_manifest_run_that_failed_on_the_chatgpt_account_says_its_own_sentence`
`verified-by: bravebot_tui::app::a_compact_that_failed_on_the_chatgpt_account_says_its_own_sentence`
`verified-by: bravebot_tui::app::a_compact_the_account_refused_says_the_wait_and_no_status`
`verified-by: bravebot_agent::chatgpt_compaction::a_summary_that_failed_on_the_account_is_narrated_in_the_accounts_sentence`
`verified-by: bravebot_agent::report::a_failed_summary_says_the_accounts_sentence_where_it_was_the_account_and_the_category_otherwise`
`verified-by: bravebot_agent::report::the_line_for_a_summary_the_account_refused_has_the_wait_and_no_status`
`verified-by: bravebot_ui_bridge::failure_tests::a_failure_of_the_account_is_sent_as_its_own_name_beside_its_category`
`verified-by: bravebot_ui_bridge::account_words::the_windows_sentence_is_one_sentence_in_sentence_case_and_names_its_command_once`
`verified-by: bravebot_ui_bridge::account_words::the_command_is_named_where_it_is_the_remedy`
`verified-by: bravebot_ui_bridge::account_words::a_wait_asked_for_at_a_429_is_said_in_minutes_for_either_refusal`
`verified-by: bravebot_agent::chatgpt::a_listing_names_the_sign_in_command_only_where_it_could_not_be_read`
`verified-by: bravebot_ui_bridge::account_words::the_notices_of_a_turn_that_goes_on_after_the_account_failed_are_sentences_of_the_window`
`verified-by: bravebot_ui_bridge::history::a_summary_that_failed_on_the_account_reaches_the_window_in_the_windows_words`
`verified-by: bravebot_ui_bridge::acp::a_summary_that_failed_on_the_chatgpt_account_is_told_to_the_editor_in_the_windows_words`
`verified-by: bravebot_tui::app::the_terminal_narrates_a_summary_that_failed_on_the_account_in_the_terminals_words`
`verified-by: bravebot_agent::turn::round_steps::a_move_to_the_fallback_after_the_accounts_failure_says_the_accounts_sentence`
`verified-by: bravebot_ui_bridge::notice_tests::a_move_to_the_fallback_after_the_account_failed_reaches_the_window_as_its_own_sentence`
`verified-by: bravebot_ui_bridge::manifest::a_run_that_failed_on_the_chatgpt_account_says_the_windows_sentence_and_its_command`
`verified-by: bravebot_ui_bridge::acp::a_turn_that_failed_on_the_chatgpt_account_tells_the_editor_which_command_signs_in`
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
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/marking.test.mjs draws the error card and the card of a run that stopped for a failure of the account and holds that the window's sentence is one paragraph with its command in a code element and no backtick, that markup in either the sentence or the command is drawn as characters, that a failure of any other service is described by its category, and that the code is set in the code type role; ui/scripts/agent-settings.test.mjs holds the sentence the bridge sends to the card's description, two sentences to two cards, and a failure with no sentence to the category's wording; ui/scripts/ux-state.test.mjs folds turn.error and a narration that names a command through the window's reducer and draws the real row, and ui/scripts/plan-card.test.mjs does the same for a run that failed; make check-ui runs them, and the governs list above holds failure.ts to existing)`
`verified-by: bravebot_aichat::client::a_second_refusal_or_a_replacement_that_cannot_be_made_ends_the_turn_as_the_sign_in_failing`
`verified-by: bravebot_aichat::client::a_refusal_naming_a_listed_code_ends_the_turn_and_replaces_nothing`
`verified-by: bravebot_aichat::client::a_listed_code_is_not_repeated_at_whatever_status_it_comes_with`
`verified-by: bravebot_aichat::client::a_rate_limit_that_names_no_listed_code_is_asked_for_again`
`verified-by: bravebot_aichat::client::a_listed_code_reported_on_the_stream_ends_the_turn_as_the_same_refusal`
`verified-by: bravebot_aichat::client::a_gateways_reply_naming_a_listed_code_is_not_the_account_being_refused`
`verified-by: bravebot_aichat::responses::protocol::the_listed_codes_are_the_seven_the_clause_names_and_the_usage_limit_is_told_apart`
`verified-by: bravebot_aichat::responses::protocol::only_a_refusals_error_code_that_is_a_listed_code_says_something_of_the_account`
`verified-by: bravebot_aichat::responses::protocol::a_listed_code_reported_on_the_stream_is_a_refusal_and_no_other_failure`
`verified-by: bravebot_aichat::client::a_model_the_account_does_not_offer_ends_the_turn_as_that_refusal_and_replaces_nothing`
`verified-by: bravebot_aichat::client::a_refusal_that_does_not_name_the_model_code_in_error_code_keeps_its_status`
`verified-by: bravebot_aichat::client::a_gateways_model_not_found_is_not_the_account_not_offering_a_model`
`verified-by: bravebot_aichat::client::a_refusal_of_a_text_over_the_maximum_length_is_the_generic_refusal_of_its_status`
`verified-by: bravebot_aichat::responses::protocol::the_code_of_a_model_the_account_does_not_offer_is_read_apart_from_the_accounts_refusals`
`verified-by: bravebot_agent::chatgpt::a_model_the_account_does_not_offer_is_said_as_a_refusal_of_the_model_and_not_of_the_sign_in`
`verified-by: bravebot_ui_bridge::account_words::a_model_the_account_does_not_offer_is_said_alike_in_the_window_and_the_terminal`
`verified-by: bravebot_ui_bridge::chatgpt_secrets::a_model_the_account_does_not_offer_reaches_the_window_as_its_own_sentence_and_touches_no_sign_in`
`verified-by: bravebot_cli::running::a_model_the_account_does_not_offer_is_reported_by_the_one_shot_run_as_one_sentence`

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

No other entry takes the name. A gateway of another id that lists a model whose own name begins `chatgpt/`, as
a LiteLLM proxy can name one, keeps that model, and the model is reached by the spelling its row has,
`<id>/chatgpt/<name>`, which sends the gateway `chatgpt/<name>`. The gateway's bare listing is not matched
against a name that begins with the identifier, so `chatgpt/<name>` selects the account, in a request and
as the account's row in a picker, whatever a gateway lists.

A Bedrock account does not take the name either. A block or a tier variable that names a model
`chatgpt/<name>` is not asked about it, since a Bedrock model is reached by the id as it stands and has no
qualified spelling to be reached by instead. The ids AWS gives, `provider.model` and an inference profile's
ARN, do not begin that way. A choice remembered under a bare name that begins `chatgpt/`, from before the
account could be signed in to, selects the account as well, and a turn on it says no sign-in is stored where
there is none.

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
   whether it takes an effort level: a row that lists none reads no effort level, and one that does not
   mention the field is not one claiming none. The levels it names are not read: the listing offers `ultra`
   for four rows, and the service refused `ultra` on `gpt-6.1-sol`, whose row lists it. The levels this
   program sends are its own five. A row in a form the reader does not expect, such as a window that is not
   a whole number, costs that row its place and keeps the rest.
3. Where the listing cannot be read, because the token could not be had, the request failed or its body is not
   the shape above, a compiled list of model names is offered instead, and the picker says the listing could
   not be read, that the list is built in and that a row may name a model the account cannot reach. A listing
   that is read and is empty offers nothing, and the compiled list is not used. A listing refused in a code
   of [CHATGPT-8](#CHATGPT-8)'s closed list is an answer and not one that could not be read: it offers
   nothing and is said in that code's sentence.

The window comes from the entry or the listing as it does for a gateway's, and the assumed low window applies
where neither states one. An output ceiling is not read from the entry, for the reason above, and no model is
left out for what it can call, since neither source says. The compiled list is a constant of the source that
names models only and takes nothing from a reply. It holds the seven models the serving host's listing offered
an account that signed in with this program, `gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-6-astra`,
`gpt-6-luna`, `gpt-6-sol` and `gpt-6.1-sol`, each of which answered a request. It holds no name ending in
`-fast`: the service refuses those as models ([Known costs](#known-costs)). Each row takes the assumed
window, which a turn on one of the account's models compacts against where the listing cannot be read, and no
label of its own, and whether it reads an effort level is what a request has already learned from the
service, as for the model an entry names. The list is one account's listing on one date, so another account
or a later date may differ.

An account whose listing is empty, where the entry names no model, is signed in. The command completes,
says the account reaches no model, and the picker has no row for it. Nothing is invented for a tier nobody named.

The roster is asked of the one function both pickers call, so the terminal and the desktop offer the same
rows. It is asked each time a picker opens, for the start that asks which of the account's models to use
([BACKEND-39](backends.md#BACKEND-39)), and otherwise only for the window of a model in force that is the
account's, so a session on another model sends the stored token nowhere. No row is offered where no sign-in is
stored, and nothing is asked of the service then. A stored sign-in whose token cannot be had, or whose listing
cannot be read, is offered the compiled list, except that the models an entry names need neither the token
nor the listing, only that the file can be read, and offer no row where it cannot be. The desktop picker says
so in a warning beside the other rosters', and the terminal picker says so in a note in the transcript,
written when the picker opens, so before it draws, at rest or mid-turn. The window's warning is a catalog
message of its own, one sentence in sentence case that says what happened and what to do, sent with the
command it names so the picker draws that command as code, and the terminal's note is lower case and names
`/model`, which the window has no use for. A gateway whose listing could not be read is said in the same
place by both pickers, the terminal's note adding the commands that name one of its models and the window's
warning saying to try again ([BACKEND-19](backends.md#BACKEND-19)). Where the compiled list is offered both say that
it is built in and may name a model the account cannot reach. A listing refused with a code of
[CHATGPT-8](#CHATGPT-8)'s closed list is said by both in a sentence of that code's own, which does not send
the person to sign in again. A person who never signed in is told nothing, and a listing that was read,
empty or not, is not a failure.

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
OpenAI's service with a person's subscription. A model a gateway lists under a name that begins with the
identifier is not a reason to send a signed-in person's turn, and the gateway's key with it, to the
gateway when the account's row of that name was picked. The gateway's own row has a name of its own, which
leaves two rows for two services as the `openai` gateway's `<name>` and the account's `chatgpt/<name>` are.
The listing is asked first because the right list is a fact about an account and a date, and a list that
waited for a release would offer a model the account cannot reach for weeks. It is the one source that says
what an account may use, as [BACKEND-19](backends.md#BACKEND-19) asks of a gateway. A compiled list stands
in only where the listing cannot be read, because a picker with no model is worse than one whose rows may be
refused at the first request, and one account's listing, every row of which answered, does not show that the
listing is dependable for every account ([What the exchange has to
record](#what-the-exchange-has-to-record)). It does not stand in for an empty listing, which is the
service's answer. Sign-in is about the account, so an account that reaches nothing today and something
tomorrow is not signed out by it. The alternative is a sign-in that fails for an account that reaches no
model.

`verified-by: bravebot_config::lib::only_the_qualified_name_selects_the_chatgpt_account_and_only_the_rest_of_it_is_sent`
`verified-by: bravebot_config::lib::a_provider_entry_named_chatgpt_keeps_the_name_and_no_model_is_the_accounts`
`verified-by: bravebot_agent::backend::a_chatgpt_model_selects_the_account_and_a_bare_name_selects_nothing_of_it`
`verified-by: bravebot_agent::backend::a_gateway_entry_named_chatgpt_keeps_its_name_and_no_model_is_the_accounts`
`verified-by: bravebot_config::lib::a_gateway_listing_a_name_that_begins_with_chatgpt_is_reached_by_its_qualified_name_alone`
`verified-by: bravebot_agent::backend::a_gateway_listing_a_chatgpt_name_does_not_take_it_from_the_account`
`verified-by: bravebot_config::lib::a_bedrock_account_listing_a_name_that_begins_with_chatgpt_does_not_take_it_from_the_account`
`verified-by: bravebot_agent::backend::a_bedrock_account_listing_a_chatgpt_name_does_not_take_it_from_the_account`
`verified-by: bravebot_cli::running::a_gateway_listing_a_chatgpt_name_does_not_take_a_turn_chosen_as_the_accounts`
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
`verified-by: bravebot_aichat::models::the_compiled_list_is_the_seven_models_the_service_offered`
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
`verified-by: bravebot_ui_bridge::account_words::each_warning_of_the_picker_is_one_sentence_and_says_nothing_where_nothing_failed`
`verified-by: by-construction (the desktop renderer is not a crate this workspace compiles, so ui/scripts/marking.test.mjs draws the picker's warnings and holds that the command a warning names is drawn as code and the rest as text, that a warning with no command or a command its sentence does not hold is a plain paragraph, and that markup in either is drawn as characters; make check-ui runs it; ui/scripts/drive-models.mjs opens the real picker against a model list with one warning and the command it names, and asserts that the command is drawn as code in the code type role and inside the menu, at the window's size and at its minimum)`
`verified-by: bravebot_cli::chatgpt::each_answer_of_the_listing_has_its_own_sentence_and_only_models_send_the_person_to_choose`
`verified-by: bravebot_tui::app::the_picker_says_why_a_signed_in_accounts_models_are_missing_and_says_nothing_otherwise`
`verified-by: bravebot_tui::app::the_notes_the_picker_says_of_the_accounts_listing_name_only_commands_that_work_as_typed`
`verified-by: bravebot_ui_bridge::models::no_warning_the_window_sends_names_a_command_line_that_names_a_model`
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
the request, which this program generated. The service's prompt cache was seen to hit with `session-id`
alone and with all three, and mostly to miss with `thread-id` alone, with `x-client-request-id` alone and
with none of them. The calls were few, so that does not show that the other two do nothing, and all three are
sent ([What the exchange has to record](#what-the-exchange-has-to-record)). The fourth is
`content-encoding: zstd`: the body is the JSON of [BACKEND-61](backends.md#BACKEND-61) compressed with zstd
into one frame, whatever its size, as the program this is read from does and as the service decodes. The
frame is what is sent, and this program sets no limit on its size: a body is sent as it
is built, so a conversation that holds a picture of several megabytes is sent. What the service refuses for
size is the service's to say. It refused a text field of 16 MiB and over, whose message states a maximum
length of 10485760, with a 400, whether the body was sent plain or as a frame. A refusal like that is
reported as any refusal of the service is, by its status, which a turn answers by shortening the
conversation once ([COMPACT-14](compaction.md#COMPACT-14)). This program adds a
`content-encoding` to the request of no other service. Compaction of a
conversation for this service is by summary ([compaction.md](compaction.md)), as it is for every service,
and the service offers none to this token: `/responses/compact` is refused.

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
skill's file or in any other way, because the service was seen to miss its prompt cache once at each change
of level inside a session. The service accepts a change of level, and holding one level is this program's
rule. A request that asks for none, as a summary, a goal check and a commit message do,
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
The service decodes the frames this program writes, so the encoding is not left to a setting, and it is
applied to every body so that no size is sent in a form the others are not. The program this is read from
compresses every body of this service, with no size above which it starts, and sends a body to another
service as it is unless a person asked for an encoding. The size of a body is left to the service, which
accepted frames of 12.6 MiB, so no limit of this program's own stops a conversation that holds a picture of a
few megabytes. The encoder is `ruzstd`, which is Rust with no build script and no native code, where the `zstd`
crate would compile the C library into each target this program is built for. libzstd decodes the frames
`ruzstd` writes, through its command line tool at 1.5.7, Node 24's `zlib` and Python 3.15's
`compression.zstd`, for bodies from empty to 8 MiB and for ones that do not compress. The service took the
frames of the turns made through this program: a text turn, a tool call, an effort level and a summary. No
other decoder has been tried. The encoder writes one level, roughly zstd
level 1, with the frame's checksum and no content size in the header.
The cache hits were seen with the identifier in `session-id`, so a session carries one and two sessions do
not share one. A record that held the identifier would tie the record on this machine to the
requests the service logged for the account, so a session resumed from one begins a new identifier, and a
cache the service still holds for the earlier requests is not found ([Known costs](#known-costs)). A level
that changes in the middle of a session was seen to miss the cache once at each change, so it is held as
the identifier is, except for a model that refuses a level and would refuse every request of the session.
The sentences say which level is sent, since a level shown as in force that is not the one
sent would mislead a person about what the session costs.

`verified-by: bravebot_aichat::client::an_account_request_carries_the_token_the_three_session_headers_and_the_zstd_encoding_and_nothing_a_gateway_would`
`verified-by: bravebot_aichat::client::a_frame_past_the_largest_the_service_took_is_sent_and_decodes_to_the_bytes_built`
`verified-by: bravebot_aichat::client::a_stream_the_host_sends_chunked_with_no_content_type_is_read`
`verified-by: bravebot_aichat::client::an_account_body_is_encoded_whatever_its_size`
`verified-by: bravebot_aichat::client::a_gateways_body_is_plain_json_with_no_content_encoding`
`verified-by: bravebot_aichat::client::a_gateways_refused_key_is_not_reported_as_the_chatgpt_sign_in_failing`
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
`verified-by: bravebot_tui::app::a_level_chosen_in_a_session_on_the_settings_model_of_the_account_is_said_to_be_kept`
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
one asks first and replaces the tokens only on a yes. An answer of no leaves the file as it is and opens no
browser.

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
listing cannot be read, the compiled names. A start that nothing serves, with the sign-in stored, asks which
of the models the listing offers to use and records the answer as `/model` does
([BACKEND-39](backends.md#BACKEND-39)). Once the sign-in
is stored the command asks the listing, as a picker does. Where the listing offers models it tells the person
to run `bravebot`, which asks which model to use, where a start would ask, and otherwise to name one in a
command that opens a session or runs a task. Where the account reaches no model, or the listing could not be
had, it says that. A person with a service of their own configured, or with a model in force that something
serves, is not asked and is not told they will be. `/model` is named only as what lists the models once a
session is open.

## Decisions left open

These are a person's to decide, and no test can settle them. The front matter's `status` stays `proposed`
while the first of them is open, although every clause is built.

- **Whether OpenAI's terms allow this.** The sign-in registers a client of its own, named `bravebot`, and
  presents no other program's identifier. Whether ChatGPT's terms allow a third-party program to use a
  subscription this way, and whether OpenAI's registration accepts this program, has to be decided before
  this ships. If it does not, this should not land as written.
- **The compiled list.** [CHATGPT-9](#CHATGPT-9) offers one where the listing cannot be read. Its contents
  are the seven models one account's listing offered, a constant a release changes. The alternative is to
  offer nothing in that case and say to name the models in the `chatgpt` entry.

## What the exchange has to record

A sign-in made with a real ChatGPT account, with the requests it makes observed, has to
establish each of the following before a clause states it as observed. What a document states is recorded in
separate lists: the key set and the discovery document, which the service publishes to anyone and which were
read with no account, and OpenAI's documentation of this sign-in. A clause that cites one states what the
document says and no more. What is recorded is a name, a shape, a status, a duration or a host. A token,
code, state, nonce, challenge, verifier, client or host identifier or claim value is never recorded, in this
repository or in the tracker.

**Observed against the real service**, as a name, a shape, a status, a size or a duration taken from sign-ins
made with a real ChatGPT account, whose plan is not recorded, on this program's own build, the latest on
2026-10-25. Requests were made
both through this program and by a script that sent the shape a clause states, and an item says "through the
program" where it was the program:

- The authorization endpoint and token endpoint on `auth.openai.com`, with the paths of
  [CHATGPT-2](#CHATGPT-2), complete a sign-in through a loopback listener on a port the operating system
  chooses. A sign-in that was not answered in the browser ended at the five minute bound with the sentence
  that the browser did not return, and stored nothing.
- The authorization server registers a client for the placeholder client and `agent_name_hint`: the callback
  carries a `client_id`, and a refresh presenting it is accepted.
- The authorization endpoint answered every request from a client that is not a browser, with any redirect
  port, with 403.
- The scopes and the `resource`, `nonce` and `ext_agent_host_id` parameters are accepted, and the reply's
  `scope` holds `chatgpt.tokens.use.direct`, since the command refuses a reply that lacks it.
- The reply to the code exchange holds an `id_token`, and this program verified a real one: a sign-in
  completes only if the algorithm, the issuer constant `https://auth.openai.com`, the audience (the issued
  client), the `nonce`, a `sub`, an `exp` and a `kid` found in the key set all hold, as
  [CHATGPT-5](#CHATGPT-5) states them. A real ID token therefore holds each. The token itself was not
  captured, so its header and its claim names are not recorded here.
- The field names of the code exchange and of the refresh are accepted, both form encoded. A reply to a
  refresh holds a new `access_token` and a new `refresh_token`. The stored expiry was 3600 seconds after the
  sign-in and 3597 after a refresh, which is `expires_in` or the default the renewal assumes where it is
  absent. The two are not told apart.
- **A refresh replaces the refresh token.** The client and host identifiers are unchanged. The claim and the
  write-before-use of [CHATGPT-7](#CHATGPT-7) are therefore required and not a precaution. Through the program,
  a refresh made by a turn whose access token was past its expiry replaced both tokens and left the other
  stored fields as they were, and the file stayed mode 0600.
- An access token minted before a refresh was still accepted after it, with a 200 on the listing.
- A refresh presenting a refresh token the server does not know, under the stored client, was refused with
  400 and the body `{"error":"invalid_grant"}`, `error` a string. An empty refresh token and a request with no
  client were refused with 400 and `invalid_request`, an unknown client with 400 and `invalid_client`, an
  unknown grant type with 400 and `unsupported_grant_type`, a code exchange with an unknown code with 400 and
  `invalid_grant`, and a refresh sent as JSON in place of a form with 400 and `invalid_grant`, since it was
  not read as a form. A `GET` on the token path was refused with 405
  and an `error` object holding `message`, `type`, `param` and `code`. No request to the token endpoint was
  answered with a redirect. A `DPoP` header holding a garbage proof changed nothing, since the refresh was
  still refused as `invalid_grant`, so no sender-constrained form was seen.
- Through the program, a refresh presenting such a refresh token, made for a turn whose access token was past
  its expiry, ended the turn with the sentence that the sign-in has ended and removed the file, and
  `auth status` then said not signed in and exited 1. An access token the serving host refused with a 401,
  under an unexpired clock, was replaced once, the request was sent again and the turn succeeded.
- The serving host refused a malformed bearer on `/responses` and on `/models` with 401 and an `error` object
  holding `message`, `type` `invalid_request_error`, `param` `null` and `code` `invalid_jwt`, and the body for
  `/responses` adds a `status` of 401. An empty bearer was refused with 401 and
  `invalid_api_key`, with a `www-authenticate` header naming the realm `OpenAI API`, and no authorization
  header with 401, a message and a null `code`.
- The listing, `GET /models` on `https://api.openai.com/v1` with the access token as the only header, is
  answered 200. The body is an object whose `models` key holds the rows: 568,400 bytes for nine rows, since a
  row carries long instruction text the picker does not read. Seven rows have `visibility` `list` and two
  have `hide`, and all have `supported_in_api` true. A row has about 55 keys, among them `slug`,
  `display_name`, `visibility`, `supported_in_api`, `context_window`, `input_modalities` (a list of words) and
  `supported_reasoning_levels` (a list of objects with an `effort` and a `description`). `context_window` is
  272000 for six rows and 500000 for `gpt-6.1-sol`. The efforts listed are `low`, `medium`, `high`, `xhigh`
  and `max`, and `ultra` for four rows.
- `POST /responses` on the serving host, with the access token as the only header, a body with `stream` `true`
  and `store` `false`, and a listed model, is answered 200 as server-sent events in a chunked body with no
  `content-type` header. The other headers seen, by name, are `set-cookie`, `x-codex-turn-state`,
  `x-models-etag`, `x-oai-request-id`, `cf-ray` and `alt-svc`. This holds for a text turn, for a turn that
  calls a function and for a turn that reasons, and every listed model answered a minimal request.
- A text turn's events are `response.created`, `response.in_progress`, `response.output_item.added`,
  `response.content_part.added`, `response.output_text.delta`, `response.metadata`,
  `response.output_text.done`, `response.content_part.done`, `response.output_item.done` and
  `response.completed`. A function call replaces the content events with
  `response.function_call_arguments.delta` and `response.function_call_arguments.done`, and a reasoning turn
  adds `response.reasoning_summary_part.added`, `response.reasoning_summary_text.delta`,
  `response.reasoning_summary_text.done` and `response.reasoning_summary_part.done` before the text.
- **The `output` list of the `response` in `response.completed` is empty.** The items arrive in
  `response.output_item.done`. The `model` of that response equals the name asked for, its `service_tier` is
  `default`, and its `reasoning` holds `context`, `effort`, `mode` and `summary`.
- A function call is an item of type `function_call` with `call_id`, `name` and `arguments`, the arguments a
  string. A `function_call_output` item with the same `call_id` and the result as text answers it, and the
  next turn is accepted with the call item as it arrived, with its `id` left off, and with a reasoning item
  from the same round left in or left out.
- Usage is `input_tokens`, `input_tokens_details` with `cached_tokens` and `cache_write_tokens`,
  `output_tokens`, `output_tokens_details` with `reasoning_tokens`, `total_tokens` and an `attribution`
  object keyed by item. `cached_tokens` is counted inside `input_tokens`: a hit read 6528 of 6677, and
  `input_tokens` did not fall.
- `instructions` is optional, and `max_output_tokens`, `tool_choice`, a strict function schema and
  `reasoning` with an `effort` and a `summary` are accepted. `reasoning.effort` was accepted at `none`, `low`,
  `medium`, `high` and `max`, and refused with 400 and `invalid_value` at `ultra`, which was sent to
  `gpt-6-luna`, whose row does not list it, and to `gpt-6.1-sol`, whose row does. The refusal lists the
  values the service takes, `none`, `minimal`, `low`, `medium`, `high`, `xhigh` and `max`.
- A request without `session-id`, `thread-id` or `x-client-request-id` is accepted. The prompt cache was
  tried by repeating one prompt of about 6.6 thousand tokens, 5 seconds apart. With all three headers it hit
  on 4 of 6 calls, the two misses being the first two. With `session-id` alone it hit on 3 of 5, with
  `thread-id` alone on 1 of 5, with `x-client-request-id` alone on 1 of 5, and with none of them on 0 of 9.
  A `prompt_cache_key` field alone hit on 0 of 1. A hit read 6528 tokens, a multiple of 128. The calls were
  few, so these figures show that `session-id` and the three together were followed by hits more than the
  others were, and do not show that `thread-id` and `x-client-request-id` do nothing. Changing
  `reasoning.effort` inside one session identifier, from low to high and back, missed the cache once at each
  change and hit again after, so the cache is per level.
- A request that is not streamed, one carrying a parameter the service does not take, one naming a tool type
  or a `service_tier` it does not take, a text over the maximum length and an effort it does not take were
  each refused with 400. The body of a refusal holds a `detail` sentence, an `error` object with `message`,
  `type`, `param` and `code`, or both, so a refusal body is not an object with one field. Only `detail`: an
  unsupported parameter (`pad`, `background`, `previous_response_id`, `temperature`, `user`), an unsupported
  tool type (`code_interpreter`, `file_search`), an unsupported `service_tier`, and a request that is not
  streamed. Only `error`: `string_above_max_length` and `invalid_value` for an effort. Both: `model_not_found`.
- The tool `image_generation` was refused with 400, the `error.code`
  `subscription_sharing_unsupported_capability`, `error.param` `tools` and a message saying that only
  `web_search` and function and custom tools are supported. `web_search` was accepted.
- Paths outside the token's rules were refused with 401. For chat completions, embeddings, files, audio
  speech, moderations, batches, vector stores, completions and fine tuning the `error.type` is
  `rejected_by_access_enforcement` and the `code` `no_matching_rule`. For `/responses/compact`,
  `/responses/input_tokens` and `/images/generations` the `type` is `rejected_by_hardened_oauth_boundary` and
  the `code` `hardened_oauth_rule_missing`. `GET /responses/<id>` was refused with 401 and `GET /models/<id>`
  with 403, each naming a missing scope (`api.responses.read`, `api.model.read`), the 403 as
  `{"error":"<sentence>"}` with a string. Some paths answered 404. There is no native compaction:
  `/responses/compact` is one of the paths refused.
- With `max_output_tokens` 16 and a long answer the stream ended with `response.incomplete`. Its
  `response.incomplete_details.reason` is `max_output_tokens`, its `status` `incomplete`, and its `output`
  holds the partial item.
- An `input_image` part holding a PNG data URL was accepted and read correctly. A model's own words were
  accepted in the history as a message of `output_text`, bare and with `type`, `status` and `annotations`.
- A request whose single text was 1.4 MB of `a `, far past the window, was answered 200 and was not refused
  for its length.
- Of the 16 names opencode offers for this sign-in, exactly seven were accepted as the `model` of a streamed
  request with 200: `gpt-5.6-luna`, `gpt-5.6-sol`, `gpt-5.6-terra`, `gpt-6-astra`, `gpt-6-luna`, `gpt-6-sol`
  and `gpt-6.1-sol`, which are the seven rows of the listing with `visibility` `list`. `gpt-5.5`,
  `gpt-5.5-fast`, the seven other `-fast` names and a made-up name were refused with 400. The two rows of the
  listing with `visibility` `hide` were accepted as well. The body of the refusal holds a `detail` and an
  `error` object, and `error` has `type` `invalid_request_error`, `code` `model_not_found` and a `message`,
  both sentences naming the model and saying it is not supported with a ChatGPT account.
- `service_tier` `priority` was accepted and `fast` was refused with 400 and a `detail` sentence. The rows of
  the listing carry `service_tiers` and `additional_speed_tiers` fields. This program sends no service tier.
- Sizes of a request. Plain bodies of 0.1 to 8 MiB with a field the service does not take reached validation
  and were refused with 400 and a `detail` sentence, and valid plain bodies of 1.2, 4 and 8 MiB were answered
  200. A single text field of 16, 32 and 64 MiB was refused with 400 and the `error.code`
  `string_above_max_length`, with the field as `error.param` and a `message` stating the maximum length,
  10485760, and so was the same text sent as a zstd frame decoding to that size. The service therefore
  decodes the frame to its full size and applies the limit to the decoded text. No limit on the size of a
  request below that was seen, and the limit of about 1 MiB for an uncompressed body that opencode states
  was not seen. Incompressible images were accepted in frames of 1.0, 2.9,
  5.6, 6.9, 8.8 and 12.6 MiB, the last holding a data URL of 16.8 MiB.
- The frames this program writes, with the frame's checksum and no content size in its header, were accepted
  on every turn made through it: a text turn, a turn that calls a function, a turn at an effort level and the
  summary of `/compact`.
- Through the program, a one-shot text turn, a turn that called `read_file`, which read 16384 tokens from the
  cache on its second round, and a turn at `--effort high` were answered 200. The terminal's `/model` picker
  offered a ChatGPT group of exactly the seven listed models, its footer took the 272000 window from the
  listing, a four-turn conversation worked, `/effort high` printed that it was thinking at high, and `/compact`
  summarised the earlier messages. The desktop bridge's `models.list` returned the same seven rows with their
  windows, 500000 for `gpt-6.1-sol`, and `vision`, and `turn.send` on one of them completed.

**Stated by the service's public documents, read with no account on 2026-10-25**, as a name, a shape, a
status or a size taken from one anonymous `GET` of each address, with no credential and no redirect followed.
A document states what the service advertises. This read made no sign-in and shows nothing about a token, a
code or a reply to a request that carried one:

- The key set, at the address of [CHATGPT-2](#CHATGPT-2), is answered 200 as `application/json`, 4080 bytes.
  The body is an object with one key, `keys`, holding a list of four entries. Every entry has `kty` `RSA`,
  `use` `sig`, `alg` `RS256`, the exponent `e` `AQAB` and a modulus of 2048 bits, and a string `kid` that is
  unique in the set. No entry has `key_ops`. Two entries also have an `x5c` list of strings and two do not.
  This shows that the set is at the address compiled in, has the shape the verifier reads, is under the
  64 KiB reply limit and holds only keys the verifier accepts. It does not show which key signs an ID token,
  that the `kid` a real token names is in the set, or that the set stays as read.
- The discovery document, at `/.well-known/openid-configuration` on `auth.openai.com`, is answered 200 as
  `application/json`, 1471 bytes. Its `issuer` is `https://auth.openai.com`, which is the constant the
  sign-in compares an ID token's issuer with. Its `authorization_endpoint`, `token_endpoint` and `jwks_uri`
  are the authorization page, the token endpoint and the key set of [CHATGPT-2](#CHATGPT-2). This shows the
  issuer the service states and the addresses it names. It does not show what an ID token's issuer is.
- It states `RS256` as the only signing algorithm for an ID token, `S256` as the only code challenge method,
  `code` as the only response type, `authorization_code` and `refresh_token` as the grant types, and
  `client_secret_basic`, `client_secret_post` and `none` as the token endpoint authentication methods.
  RS256 is the algorithm the verifier accepts, `S256` is the method the sign-in sends, and the two grant
  types are the two requests the sign-in makes. The document advertises `none` as a method, and the list of
  OpenAI's documentation below records that OpenAI states this direct flow needs no client secret. The
  method the client registered for this program uses is not observed.
- Its `scopes_supported` is `openid`, `profile`, `email` and `offline_access`. It does not list
  `resource.invoke` or `chatgpt.tokens.use.direct`, which the sign-in asks for, so it neither supports nor
  contradicts them. What the service accepted of them is in the list above.
- Its `claims_supported` is `sub` and sixteen profile and email claims. It lists none of `iss`, `aud`, `exp`
  and `nonce`, which with `sub` are the five claims the sign-in compares. A list of the claims a service can
  supply does not say which claims every token carries, so it says nothing about whether a real ID token
  carries any of those four.
- It lists a `revocation_endpoint` on `auth.openai.com`, at the path `/api/accounts/oauth/revoke`, and lists
  no introspection endpoint. It has no field that names a sender-constrained token form: no DPoP field and
  no mutual-TLS field. Its discovery document therefore advertises a way to
  revoke and advertises no sender-constrained form. The revocation endpoint shows nothing of what revoking
  does to an access token already issued. An absent advertisement does not show that a request for a
  sender-constrained form is refused, and none was made.
- The address `/.well-known/oauth-authorization-server` on `auth.openai.com`, where RFC 8414 puts the
  metadata of an authorization server, is answered 200 as `text/html`, about 57 KB, with a page that holds
  no metadata.

**Stated in OpenAI's documentation of this sign-in, read on 2026-10-25.** The documentation is the pages of
`developers.openai.com/siwc` named "Registration and sign-in", "Accounts and sessions", "Token reference",
"Errors and recovery", "Models and inference" and "Preview limitations". "Registration and sign-in" names a
seventh page, "On your website", for the signature check of an ID token. Each item is what a page states,
and a statement shows nothing about what the service does for this program's build. Where this program
differs from a statement, the item or an entry of [Known costs](#known-costs) says so:

- Issuer. "Registration and sign-in" holds `https://auth.openai.com` as the `issuer` of its example
  credential record, beside the validated subject. "On your website" says to require the ID token's `iss`
  claim to match exactly the issuer returned by discovery, and lists the production issuer, authorization
  endpoint, token endpoint and key set address. The last three are the addresses of [CHATGPT-2](#CHATGPT-2),
  and the issuer is the one the discovery document states. "Token reference" gives `iss` as
  `https://auth.openai.com` in production for the claims of the access token, which is a different token
  from the ID token. An ID token's `iss` is therefore documented to equal the issuer of the discovery
  document, and no page records the `iss` of a real ID token.
- What a client checks. "Registration and sign-in" says to verify the ID token's signature against OpenAI's
  published key set and to check the issuer, the audience against the issued client, the expiration and the
  nonce saved for the attempt, and to use the validated `sub` as the account identity. For a returning
  account it says to confirm that the new ID token's verified identity matches the selected account before
  credentials are replaced. "On your website" shows the same checks in an example that also requires the
  claims `sub`, `exp` and `iat` to be present, allows a clock tolerance of five seconds, and says to refresh
  the key set when an unfamiliar `kid` appears. [CHATGPT-5](#CHATGPT-5) compares the signature, issuer,
  audience, expiry and nonce, which are the same checks, and reads `sub` only as a string that is not empty.
  Known costs says where it differs from the rest.
- Redirect. "Registration and sign-in" says the redirect URI is an HTTP loopback address on `127.0.0.1` from
  initial registration onward, with the path `/auth/callback`. Its example names a port. It says later
  sign-ins may use another available port, that the scheme, host and path stay unchanged, and that
  `localhost` is not substituted. It does not say whether registration accepts a port other than its
  example's. The list above from a sign-in records only that a port the operating system chose was accepted
  at a first sign-in.
- Client authentication. "Registration and sign-in" says this direct flow is a public client and needs
  neither a client secret nor a partner API key. "On your website" says an identity-only public client
  authenticates at the token endpoint with `none`.
- Tokens. "Token reference" lists the fields of the token response as `access_token`, `refresh_token`,
  `id_token`, `token_type`, `expires_in`, `scope` and `earliest_refresh_at`. It gives an access token one
  hour (`expires_in` 3600) and a refresh token 30 days. It says a successful code exchange is answered 200,
  and so is a successful refresh, with a new access token and a replacement refresh token that has a fresh
  30 days. The example credential record of "Registration and sign-in" gives the token type as `Bearer`, and
  "Models and inference" sends the access token as `Authorization: Bearer`. No page mentions a
  sender-constrained token form.
- Sign-out. "Accounts and sessions" describes a revocation call for sign-out: a form-encoded `POST` to the
  `revocation_endpoint` of the discovery document with `token` the refresh token, `token_type_hint`
  `refresh_token` and the issued client. An empty 200 is its success reply, including for a token that is
  already invalid. It says to retry on a network failure or a 5xx, to clear the tokens locally and tell the
  person that remote revocation was not confirmed if sign-out finishes without confirmation, and that a
  person can disconnect the app in ChatGPT Settings. It says nothing of an access token already issued.
- Error codes. "Errors and recovery" lists seven codes, each with the HTTP status it gives:
  `subscription_sharing_user_not_eligible` 403, `subscription_sharing_usage_limit_exceeded` 429,
  `subscription_sharing_unsupported_capability` 400, `subscription_sharing_route_not_supported` 403,
  `subscription_sharing_invalid_user` 401, and `chatpass_v2_scope_not_authorized` and
  `chatpass_v2_invalid_authorization_context` 403. It lists two more,
  `subscription_sharing_usage_unavailable` and `subscription_sharing_user_unavailable`, each 503 with a
  retry under a bounded backoff. The list of [CHATGPT-8](#CHATGPT-8) leaves both out, and Known costs says
  how this program reads them. It says a Responses-layer error object carries a code as `error.code` and a
  parameter as `error.param`. It says direct-route admission can return a body such as `{"detail": "..."}`
  before a stream opens, with 401, 403 or 503, and that its message is diagnostic text and no stable code. A
  401 there means the required signed identity or direct permission was not accepted, and the page does not
  say whether that covers an expired or revoked access token. The prose of "Models and inference" says a
  usage-limit error can arrive after streaming has begun as `response.failed`, with
  `subscription_sharing_usage_limit_exceeded` or `subscription_sharing_usage_unavailable`, and its Python
  example reads the code as `event.response.error.code`. No page names an `error` event, so `code` in an
  `error` event, which [CHATGPT-8](#CHATGPT-8) reads, is stated by no documentation.
- Refresh errors. "Errors and recovery" lists `invalid_grant` on refresh, `invalid_refresh_token`,
  `token_expired`, `refresh_token_expired`, `refresh_token_invalidated` and `refresh_token_reused` as an
  unusable refresh token, and says to clear the tokens and repeat OAuth with the saved client. It lists
  `invalid_client` as a fault in the client's configuration. It gives no HTTP status for any of these codes.
- Requests and streams. "Preview limitations" says a request sets `store` to false and `stream` to true,
  sends `input` as an array, and uses `instructions` or developer messages, since explicit system-role
  message items are rejected. It lists fifteen fields to omit: `background`, `conversation`,
  `max_output_tokens`, `max_tool_calls`, `metadata`, `moderation`, `multi_agent`, `prompt`,
  `prompt_cache_retention`, `safety_identifier`, `temperature`, `top_logprobs`, `top_p`, `truncation` and
  `user`. It says text, images and files are supported when the selected model accepts them, and this
  program drops a document attachment from a Responses request, as [BACKEND-61](backends.md#BACKEND-61) says
  ([#3121](https://github.com/brave/bravebot/issues/3121)). "Models and inference" says to treat inference
  as successful only after `response.completed`, and to handle `response.incomplete`, explicit errors and
  interrupted streams apart from it. It does not say when the host sends `response.incomplete` or ends a
  stream early.
- Model listing. "Models and inference" says to request the account's model catalog with the same access
  token that inference uses, and that the response contains a `models` array. Its example is a `GET` of
  `/v1/models` that keeps the rows whose `visibility` is `list`. It says to show `display_name`, to pass the
  selected `slug` as `model`, and to refresh the choices when the person switches accounts. This matches the
  listing of [CHATGPT-9](#CHATGPT-9) and the list above from a sign-in, except that the example keeps a row
  on `visibility` alone, where CHATGPT-9 also requires `supported_in_api`. Nothing there says how dependable
  the listing is.
- Host identifier and client. "Registration and sign-in" says to save the issued client with the
  registration for the account and reuse it at later sign-ins, and to send the same host identifier for the
  same host. At reauthorization it says to omit `agent_name_hint`, to send the ID token retained from the
  account's previous successful sign-in as `id_token_hint` and to omit it after signing out or if none was
  retained, and that the email saved from the validated ID token may be sent as `login_hint`. "Accounts and
  sessions" says signing out or switching back to a saved account does not create a new client. It says to
  clear the selected account's access, refresh and ID tokens at sign-out, to retain its account and client
  mapping and the host's identifier for a later sign-in, and that revoking a session does not delete the
  registered client.

**Stated by the clauses from opencode's source, and not confirmed:**

- That the authorization server accepts a redirect to `http://127.0.0.1:<port>/auth/callback` for any port.
  A port the operating system chose is accepted and no other is confirmed. Another port cannot be tried
  without a browser, as the observed list records. OpenAI's documentation states a redirect rule, which is
  listed above and confirms no port.
- The error codes that end the use of a subscription, other than
  `subscription_sharing_unsupported_capability`, which the observed list holds:
  `subscription_sharing_usage_limit_exceeded`, `subscription_sharing_user_not_eligible`,
  `subscription_sharing_route_not_supported`, `subscription_sharing_invalid_user`,
  `chatpass_v2_scope_not_authorized` and `chatpass_v2_invalid_authorization_context`, and where they are
  written: `error.code` in the body of a refusal, as opencode's tests have it for a 429,
  `response.error.code` in a `response.failed` event and `code` in an `error` event. OpenAI's documentation
  lists the codes with their statuses, and no documentation states `code` in an `error` event. No request
  made with this account provoked any of the six, a `response.failed` event or an `error` event.

**Not observed, and stated by no source in full:**

- What an account with no subscription reaches. The plan of the account tried is not recorded.
- Whether an access token minted before a revocation survives it. The discovery document lists a
  revocation endpoint, which says nothing of a token already issued.
- Whether the service issues a sender-constrained form to a request for one. The discovery document
  advertises no sender-constrained form, which does not show that a request for one is refused.
- Whether the listing is dependable for another account or on another date. A listing that includes a model
  the account cannot reach was not seen.
- Whether OpenAI limits the clients registered for one host identifier, and what a second sign-in on a
  machine that has signed out registers.

## What this changes elsewhere

These are the places where another spec states something about this service, each written in that spec.

- **Backends.** [BACKEND-3](backends.md#BACKEND-3) and [BACKEND-18](backends.md#BACKEND-18) gain the
  `chatgpt/` identifier. [BACKEND-9](backends.md#BACKEND-9) gains a note that this service starts no
  sign-in. [BACKEND-39](backends.md#BACKEND-39) names four ways to configure a service, a stored
  ChatGPT sign-in among them. [BACKEND-13](backends.md#BACKEND-13), [BACKEND-17](backends.md#BACKEND-17)
  and [BACKEND-5](backends.md#BACKEND-5) each gain the case of [CHATGPT-9](#CHATGPT-9): an entry named
  `chatgpt` that states no endpoint is read for `models`, where an entry of any other name with no endpoint
  configures no service. [BACKEND-19](backends.md#BACKEND-19) gains this service's listing and its
  absence. The cases of [CHATGPT-10](#CHATGPT-10) land in [BACKEND-3](backends.md#BACKEND-3) to
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
- **Network egress.** [network-egress.md](network-egress.md) names the destinations of this flow by
  pointing at [CHATGPT-2](#CHATGPT-2) and holds no copy of the addresses. Its register of the crates that
  keep an HTTP client of their own records that `bravebot-skus` builds one for the subscription service
  and one for this sign-in, because a request there carries no labelled value for the gate in
  `bravebot-net` to judge, and that the code exchange, the key set and the renewal of an access token go
  through them over the transport the caller states. This sign-in's clients follow no redirect, and
  what the subscription client does with one stays undecided there. A turn and a listing go through the
  one way out of [NET-1](network-egress.md#NET-1) with a request that refuses redirects
  ([NET-2](network-egress.md#NET-2)) and is handed the body of a refusal
  ([NET-12](network-egress.md#NET-12)). The register's cost about a hop within https has an exception for
  a request to the serving host, which carries the access token and follows no redirect, since a person
  did not choose that host. That spec's scope is requests that leave this process, so the listener,
  which accepts connections from any program on this machine, answers each with a fixed page and makes
  no request, is governed by [CHATGPT-4](#CHATGPT-4).
- **Labels.** No exception is added to [labels.md](labels.md): [CHATGPT-5](#CHATGPT-5) shows the flow
  holds the rule. The counts that spec pins moved with the body of a refusal that a request may ask
  for ([NET-12](network-egress.md#NET-12)): `Labelled::new` in the egress crate went from 2 to 4, for
  the place that labels the body handed back and a test that builds a failure carrying one, and
  `Policy::decode_transport` went from 2 to 3 in the aichat crate's `lib.rs`, for the one place the body
  is read, and from 2 to 3 in the egress crate's `tests/egress.rs`, for a test.

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
  needs one. A turn can also be refused once for a token that a clock running behind made look valid. A
  clock set before 1970 reads as 1970 where a renewal compares an expiry, so every token looks valid and the
  first request the service refuses replaces it. The renewal stores an `expires_at` of the token's lifetime
  past 1970, near 3600, so the next start under a correct clock finds the token expired and renews it again.
  A test renews under a clock that reads no time and checks the file holds the reply's lifetime as the
  expiry, then starts a second renewer under the system clock and checks it renews again. The sign-in
  itself refuses such a clock ([CHATGPT-5](#CHATGPT-5)).
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
  person who wants another types `chatgpt/<name>`. The names are the seven one account's listing offered on
  one date, so another account may be offered others and one may name a model the account cannot reach. The
  service refuses it at the first request with `model_not_found`, and the turn says that the account does not
  offer that model and that another can be chosen, without naming the model ([CHATGPT-8](#CHATGPT-8)).
- The names ending in `-fast`, which opencode offers as models, are not offered, since the service refuses
  them as models. The service accepts the `service_tier` `priority` and refuses `fast`, and this program sends
  no service tier. Whether the `-fast` names stand for the `priority` tier was not observed. Offering a tier
  takes a request field and a way for a person to choose it, and neither is built.
- The egress layer reads at most 2 MiB of a reply, and the listing is 568,400 bytes for nine rows,
  so an account whose listing passes 2 MiB is read as one that cannot be read and is offered the compiled
  names.
- Where an entry names models ([CHATGPT-9](#CHATGPT-9)), a model named directly that the entry does not name
  has no row, so a turn on it compacts against the assumed window and not the one the listing would give.
  Stating the model in the entry, with its `limit`, gives it one.
- One account is signed in at a time on one machine, as one subscription is imported at a time.
- A person whose gateway entry already has the id `chatgpt` renames it to sign in.
- A gateway model whose name begins `chatgpt/` is reached by its qualified spelling, `<id>/chatgpt/<name>`,
  and a choice remembered by the bare name selects the account until the person picks the gateway's row
  again ([CHATGPT-9](#CHATGPT-9)). A Bedrock model listed under such a name cannot be chosen at all, since
  it has no qualified spelling.
- The body is encoded at the one level `ruzstd` writes, which makes frames about 1.4 times the size of
  libzstd's default level on request-shaped JSON (0.40 of the input against 0.27 to 0.29, measured on bodies
  built from this repository's files). It matches a repeat only within 128 KiB, so a conversation that holds
  the same large text twice, far apart, gains nothing from the second copy.
- A picker asks the serving host for the listing each time it opens, the sign-in command asks it when the
  sign-in is stored, and a start that nothing serves asks it before it asks which model to use, so the
  access token is sent to the serving host in requests that are not a turn.
- A sign-in that ended while no picker was open is found when the picker opens, which says nothing about it,
  since the renewal that finds it removes the file. The next turn on a `chatgpt/` model in that process says
  so ([CHATGPT-7](#CHATGPT-7)). A start that asks which model to use finds it as well, and says it has
  ended. A turn in another process finds no file and says no sign-in is stored.
- A session resumed from a record, and a fork begun from one, carry an identifier of their own, so the first
  request after either does not find a cache the service still holds for the earlier session's requests.
- A model of the account that refuses the level ends it for that model for the rest of the process, so a
  session that began at a level is sent none by that model, and so is every session after it. A body
  refused for another reason that does not recur is taken for a refusal of the level when it is taken
  without it.
- A session sends the account two levels. The rounds of its turns, and the planning calls of a manifest run,
  carry the level it began with, and a summary, a question beside the work, a goal check, a commit message,
  a processor, a vetting and an advisor ask for none, so the service applies its own default to them. The
  service was seen to miss its cache once at a change of level and to hit again after, so a request that asks
  for none may miss the cache the rounds hold. Whether the round after it misses is not observed.
- A level chosen in a session on a model of the account applies from the next session, so a person who
  wants another level for the account begins one with `/clear` or by starting again. A session that began
  on a model that reads no level was begun at the level chosen all the same, and the account is sent it.
- A skill's file that asks for a level changes nothing where the account answers, in every front end, and
  the desktop is told so in the notice a skill gets although it offers no way to choose a level.
- The codes that say the account has reached its usage limit or may not be used this way, and the fields of
  a refusal and of a failed event that hold them, are read from another program's source, are listed with
  their statuses in OpenAI's documentation, and one of the seven,
  `subscription_sharing_unsupported_capability`, was seen from this service, as the `error.code` of a 400. The
  other six, and the fields of a failed event, were not. A refusal in a reply that says it in another code or
  another field is handled as its status: a 401 is replaced once, a 429 or a 5xx is asked for again, and the
  turn ends with the sentence of that status. The two codes that program
  retries (`subscription_sharing_usage_unavailable` and `subscription_sharing_user_unavailable`) are not in
  the list. OpenAI's documentation lists both with the status 503 and a retry under a bounded backoff. A 503
  reply is sent again, up to three requests in all with a pause that doubles from one second. A failed event
  of a stream answered 200 has no status, and either code in one is read as an unknown failure, which is not
  asked for again and is reported in the incomplete category with nothing of what the service said. The
  documentation says a usage-limit error can arrive that way after streaming has begun. It names no `error`
  event, so the `code` this program reads in one is stated by no documentation.
- `subscription_sharing_unsupported_capability` was seen caused by the tool `image_generation`, which this
  program never sends, and the message of the refusal named the tool. The sentence for the code is chosen
  from the code alone, so it does not say the cause, and the window's adds to use another account or another
  service's model. Where the cause is in the request and not in the account, that advice is wrong.
- "Errors and recovery" gives six of the seven codes different recoveries. For
  `subscription_sharing_unsupported_capability` it says to inspect `error.param`, remove the unsupported
  input, tool, execution feature, model or service-tier override, and not retry the same body. For
  `subscription_sharing_route_not_supported` it says to check the exact HTTP method and endpoint. For
  `subscription_sharing_invalid_user` it says to keep the request ID and diagnose the credential failure,
  and to ask the person to sign in again after a confirmed revocation or a terminal refresh error. For
  `subscription_sharing_user_not_eligible` it says to explain the restriction and not to repeat the same
  request or loop through OAuth. For the two `chatpass_v2` codes it says to check the client and grant
  configuration and not to retry or change billing. [CHATGPT-8](#CHATGPT-8) gives the six one sentence, that
  the account may not be used this way, which names no cause, does not name signing in again as the remedy
  and shows nothing from `error.param`.
- The sentences of [CHATGPT-8](#CHATGPT-8) are written in two sets in the catalog: the terminal's, which the
  one-shot run and the goal and manifest runs say too, and the window's. A change to what a failure says is
  made in both, and no test holds the wording equal since the two differ in form. A test holds that the
  command each names is the one the other names, for every failure of the account including the failed
  renewals, and for each way the account's models could not be listed. The window keeps no table of its
  own. The twelve names are listed in `ui/docs/phase-0-rpc-protocol.md`.
- The code that says a refresh token is no longer valid is RFC 6749's. This server sent it for a refresh
  token it did not know, and no refresh token that had ended by age, by revocation or by reuse was presented,
  so one of those may come in another code. A server that ends a sign-in with another code leaves the file in
  place, and each later turn tries the dead refresh token and is told the refresh failed, until the person
  signs in again. OpenAI's documentation lists `invalid_grant` on refresh and five more codes for an
  unusable refresh token: `invalid_refresh_token`, `token_expired`, `refresh_token_expired`,
  `refresh_token_invalidated` and `refresh_token_reused`. The renewal reads only `invalid_grant`, and only
  at a 400 or 401, which the documentation gives for no code. A refresh refused with any of the five, or
  with `invalid_grant` at another status, is the case this entry describes. The documentation lists
  `invalid_client` for a fault in the client's configuration, which the renewal treats as any other refusal.
- A code exchange whose reply holds no ID token ends the sign-in, so a server that stopped sending one, or
  that named another issuer, would refuse every sign-in until a release changes the check. The expiry is
  compared with the clock with no allowance, so a clock running ahead of the server's by more than the
  token's lifetime refuses the sign-in. The example in OpenAI's documentation that verifies an ID token allows
  a clock tolerance of five seconds.
- The example in OpenAI's documentation that verifies an ID token requires the claims `sub`, `exp` and `iat`
  to be present, and refreshes the key set when it meets a `kid` it does not hold. The verifier of
  [CHATGPT-5](#CHATGPT-5) reads no `iat` and fetches the key set once per sign-in, so an ID token whose
  `kid` the set does not hold is refused.
- The binary `cargo test -p bravebot-cli` builds, and one `cargo build --all-targets` builds, is a debug
  build with the `test-hosts` feature, so in it `BRAVEBOT_TEST_CHATGPT_STAND_IN` sends the sign-in and its
  tokens where it says. A person who runs that binary from `target/debug` as their own carries the seam.
  `cargo test --workspace` gives the feature to every workspace binary it builds, and `cargo test --release`
  and `cargo build --release --all-targets` stop at the guard that refuses a release build with it.
- The guards of the `test-hosts` seam read text. They do not read a command kept under `docs/specs`, in a Rust
  source (a `build.rs`, or a test that runs `cargo`), in a file with a NUL byte, in a directory named
  `target`, `node_modules`, `.git`, `.worktrees`, `__pycache__`, `dist`, `out`, `build` or `.docusaurus` (a
  manifest under `target` is not read either), or outside the repository, such as a person's
  `~/.cargo/config.toml` or a variable set on a build machine, and they do not see a command that builds a
  name or a flag when it runs, such as `f=test-; cargo build --features "${f}hosts"`. A release built that way
  carries the seam without a test of this repository failing, if it also turns debug assertions on and makes a
  panic unwind, since the refusal at compile time stops a build with either of them as a release has them.
  Nobody has built one. The guards run with the Rust checks. A change to a file they name and count lines in
  (`contrib/affected-checks.py` lists them) runs those checks whatever else it is, and a command added to a
  file they only scan, in a change that touches no Rust, is found when the Rust checks next run.
- A request to the account carries none of the fifteen fields OpenAI's documentation lists to omit. It is
  built from `model`, `input`, `stream` and `store`, with `instructions`, `tools`, `tool_choice` and
  `reasoning` where they apply. The request type can carry `max_output_tokens` for another service where a
  ceiling is stated for the model, and none is stated for the account: its provider names no model and holds
  no budget, and [CHATGPT-9](#CHATGPT-9) reads no output ceiling from the `chatgpt` entry. The list from a
  sign-in above records `max_output_tokens` accepted, so the documentation and that list differ on it, and a
  person cannot set a ceiling for a model of the account whichever is right.
  [BACKEND-41](backends.md#BACKEND-41) says one exported figure states a ceiling for every model this build
  reaches. The figure does not reach the account's models, since it is applied to the providers a `provider`
  block lists and to Bedrock and the account's provider is built with none, so the account is an exception
  that [backends.md](backends.md) does not state.
- OpenAI's documentation says to keep the issued client and the host identifier for an account across a
  sign-out and reuse them at the next sign-in, to send an ID token as `id_token_hint` only while one is
  retained from a previous sign-in and not after a sign-out, that the saved email may be sent as
  `login_hint`, and to confirm that a returning account's new ID token names the account selected. This
  program keeps the client and the host identifier in the sign-in file and presents both at a sign-in over a
  stored one ([CHATGPT-3](#CHATGPT-3)). `auth logout chatgpt` removes the file ([CHATGPT-11](#CHATGPT-11)),
  so the next sign-in generates a new host identifier and asks OpenAI to register a client. It stores no ID
  token and no subject ([CHATGPT-6](#CHATGPT-6), [CHATGPT-5](#CHATGPT-5)), sends neither hint, and makes no
  comparison of a returning account's ID token with an earlier one. Whether OpenAI limits the clients
  registered for one host identifier is not stated.
- OpenAI's documentation describes a revocation call a client makes at sign-out, which revokes the refresh
  token. This program makes no such call. `auth logout chatgpt` removes the copy here and ends nothing at
  OpenAI ([CHATGPT-11](#CHATGPT-11)). [CHATGPT-6](#CHATGPT-6) records two ways to end the credential at
  OpenAI, and the program does neither for the person. The documented revocation call is a third way that
  CHATGPT-6 does not record. The documentation says nothing of an access token already issued, so whether
  one survives a revocation is not observed.
- Four more statements of the documentation describe what this program does otherwise. "Errors and recovery"
  says a valid ID token without `chatgpt.tokens.use.direct` is to be kept, with ChatGPT plan usage marked
  disabled and a way offered to enable it or to use another billing path. A reply without the scope ends the
  sign-in before anything is stored, and the command ends with a sentence that names signing in again and
  the `openai` gateway ([CHATGPT-8](#CHATGPT-8)). For `subscription_sharing_usage_limit_exceeded` the page
  says not to assume the whole plan is empty or to infer a reset time, since an app-specific limit can also
  apply, and to link to ChatGPT settings, Usage. This program's sentence says the account has reached its
  usage limit, gives a wait only where a 429 reply asked for one, and links to no usage page. "Preview
  limitations" says to group function and custom tools in namespaces or supply them as `additional_tools`
  input items. This program sends each tool as a flat `function` object in `tools`, and the list from a
  sign-in above records a turn that calls a function answered 200. The
  documentation's example credential record keeps `email`, `scopes` and the ID token among other fields, and
  this program's sign-in file holds a version, the two tokens, the expiry, the client and the host
  identifier and no other field ([CHATGPT-6](#CHATGPT-6)).
- Whether a third-party program may use a subscription this way is not this spec's to settle
  ([Decisions left open](#decisions-left-open)). The design assumes a person is allowed to, and an answer of
  no ends it.
