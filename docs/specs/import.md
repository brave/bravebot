---
id: IMPORT
title: Importing a model service from another agent
status: normative
governs:
  - crates/config/src/import.rs
  - crates/aichat/src/ollama.rs
  - crates/aichat/src/lm_studio.rs
  - crates/aichat/src/llama_server.rs
  - crates/cli/src/import.rs
  - crates/cli/src/main.rs
  - crates/cli/src/invocation.rs
  - crates/cli/src/flags.rs
  - crates/cli/src/model.rs
  - crates/cli/src/doctor.rs
  - crates/cli/src/plain.rs
  - crates/cli/src/session_import.rs
  - crates/cli/src/background.rs
documented-by:
  - docs/website/docs/customize/configuration.md
  - docs/website/docs/customize/importing.md
  - docs/website/docs/customize/providers/ollama.md
  - docs/website/docs/customize/providers/lm-studio.md
  - docs/website/docs/customize/providers/llama-server.md
---

## Scope

What bravebot reads from Claude Code's and opencode's configuration and from the API keys exported
in its own environment, and asks a local Ollama, LM Studio or llama-server, when it starts with no
service configured to answer. It also covers which of those settings may become bravebot's own
configuration, how the person is asked, and what is written. Of the six sources, two are files, one
is the process environment, and the other three are servers running on this machine, each asked what
it serves. The refusal a start gets with nothing configured, and the routes it names,
are [backends.md](backends.md)'s.

It covers model services only: the Bedrock account and the OpenAI-compatible gateways
[backends.md](backends.md) already defines. It adds no new service.

It never makes bravebot read another tool's files or setting names at run time. An import is a
one-time copy a person approved, written in bravebot's own spelling. After it, the other tool's
files are not read again, and the server is asked only for its roster, as any gateway is.

It also covers the one other thing copied from these tools: a session's words, when a person runs
`bravebot sessions import` ([IMPORT-11](#IMPORT-11)).

Out of scope: permissions, hooks, MCP servers, instructions, skills, and the desktop app's first
run.

## Clauses

<a id="IMPORT-1"></a>
### IMPORT-1: a start with nothing configured offers what Claude Code and opencode configured, a key exported for a known service, or what a local Ollama, LM Studio or llama-server serves, where bravebot can use it

The start looks for the [IMPORT-2](#IMPORT-2) sources before the session opens when all of these
hold:

- the start would be refused for naming no model service;
- no service is configured at all, which is the case the refusal answers with the routes, not
  the one where a configured service lacks only a model name;
- stdin, stdout and stderr are all terminals. The question is written to stderr and answered on
  stdin, and the session it leads to draws on stdout.

If a source holds something [IMPORT-3](#IMPORT-3) accepts, the start asks whether to import it
before it says anything else. If none does, the start is refused as it was before this spec.

The interface that draws and `--plain` ask in the same way, one line at a time, because the
question comes before anything is drawn.

The case where a service is configured and names no model gets no offer: what is missing there is
one key, which the refusal already names.

A decline is not recorded. The start is then refused as before, and the next start asks again,
because nothing on the machine has changed.

**Why.** Somebody arriving from one of these tools has already written this configuration once. The
routes ask them to write it again by hand, in a shape bravebot borrowed from those same tools.
The offer comes only where the answer is "configure a service", because a start that already works
should not open with a question about another program.

`verified-by: bravebot_cli::running::a_first_run_with_claude_code_configured_offers_to_import_it`
`verified-by: bravebot_cli::running::a_first_run_with_an_exported_key_offers_an_entry_naming_the_variable`
`verified-by: bravebot_cli::running::a_first_run_with_a_claude_code_key_command_writes_an_entry_naming_the_variable`
`verified-by: bravebot_cli::running::a_first_run_with_ollama_running_offers_to_import_it`
`verified-by: bravebot_cli::running::a_first_run_with_nothing_importable_refuses_as_before`
`verified-by: bravebot_cli::running::a_configured_service_with_a_brave_model_is_not_offered_an_import`
`verified-by: bravebot_cli::running::nothing_is_asked_where_stdin_or_stdout_is_not_a_terminal`

<a id="IMPORT-2"></a>
### IMPORT-2: only a person's own user-level configuration is read, never a checkout's

**Claude Code.** Three sources are read:

- `settings.json` in `$CLAUDE_CONFIG_DIR`, which defaults to `~/.claude`. Nothing else in that
  directory is opened: not `.credentials.json`, and not the macOS keychain entry.
- `CLAUDE_CODE_USE_BEDROCK` in the process environment, because that is where a Claude Code Bedrock
  setup is often exported.
- `CLAUDE_CODE_USE_VERTEX`, `CLAUDE_CODE_USE_FOUNDRY` and `ANTHROPIC_BASE_URL` in the process
  environment, which Claude Code reads as it reads the settings file's `env`. They are read only for
  whether they are set, and are written nowhere. Each means a key Claude Code reaches a service with
  was not issued for Anthropic's API ([IMPORT-3](#IMPORT-3)).

**opencode.** Three sources are read, merged the way opencode merges them:

- `opencode.json` and `opencode.jsonc` in `$XDG_CONFIG_HOME/opencode`, which defaults to
  `~/.config/opencode`;
- the file `OPENCODE_CONFIG` names;
- `auth.json` in `$XDG_DATA_HOME/opencode`, which defaults to `~/.local/share/opencode`.

A relative path in any of those variables is never resolved, since it would name a file in
whichever directory bravebot started in. A relative `XDG_CONFIG_HOME` or `XDG_DATA_HOME` is invalid
by the XDG rule and its default applies; a relative `CLAUDE_CONFIG_DIR` or `OPENCODE_CONFIG` is
read nowhere, because the person pointed the tool somewhere other than the default. A checkout's
`.claude/`, `opencode.json` and `.opencode/` are never opened. A machine whose platform names no
profile directory reads nothing.

**Exported keys.** The process environment is read for the variable [IMPORT-3](#IMPORT-3) names for
each service whose endpoint is compiled in (`OPENROUTER_API_KEY`, `OPENAI_API_KEY` and
`ANTHROPIC_API_KEY`), and for the variables that service's own tools read a host from
(`OPENAI_BASE_URL`, `OPENAI_API_BASE` and `ANTHROPIC_BASE_URL`). Only whether one is set is read. The
value is held as a secret for that check, cleared, and never kept, shown or written. A variable that
is set and blank is not set. No file is opened for this source.

**Ollama.** One server is asked, at the address `OLLAMA_HOST` names, read the way Ollama's own
client reads it: `[scheme://]host[:port][/path]`. The defaults are `http` and port `11434`, or
port 80 or 443 where `http` or `https` is written with no port. A `0.0.0.0` or `::` bind address
means this machine. Unset or empty, it is `http://localhost:11434`. Only an address on this machine
is asked: `localhost`, a loopback address, or a bind address. Any other value is left
([IMPORT-4](#IMPORT-4)) under the name `OLLAMA_HOST`, and no request is made. What is asked, and
when, is [IMPORT-10](#IMPORT-10)'s.

**LM Studio and llama-server.** Each is asked on this machine, by the rule above.

- LM Studio is asked at the address `BRAVEBOT_LM_STUDIO_HOST` names, read as `OLLAMA_HOST` is, with
  port `1234` where it names no scheme and no port. Unset or empty, it is `http://127.0.0.1:1234`.
  A value naming an address other than this machine's loopback is left under the name
  `BRAVEBOT_LM_STUDIO_HOST`.
- llama-server is asked on the address `LLAMA_ARG_HOST` names and at the port `LLAMA_ARG_PORT`
  names. Unset or empty, the address is `127.0.0.1`, and `0.0.0.0` and `::` mean this machine. A
  port that is unset, is not a port number or is `0` leaves the two ports llama-server defaults to,
  `9931` and `8080` ([IMPORT-12](#IMPORT-12)). An address that is not `localhost` or a loopback or
  bind address, including a list of addresses and a socket path, is left under the name
  `LLAMA_ARG_HOST`, and no request is made.

What is asked of each, and when, is [IMPORT-12](#IMPORT-12)'s.

**Why.** Home-level files are the person's own configuration, on the same footing as
`~/.bravebot/settings.json`. A checkout's files hold whatever the repository's author wrote.
Importing a host and a credential name from them would let a clone decide where the person's key is
sent. `OLLAMA_HOST` is honoured on the footing `CLAUDE_CONFIG_DIR` is: it is the variable the other
program itself reads, as are `LLAMA_ARG_HOST` and `LLAMA_ARG_PORT` for llama-server. LM Studio reads
no variable for where it listens, so `BRAVEBOT_LM_STUDIO_HOST` is bravebot's own and moves only where
bravebot asks, for a person whose LM Studio listens on another port. Only this machine is asked, so
that no request leaves it before a question.
A proxy the environment names carries this request as it carries every other
([NET-8](network-egress.md#NET-8)), unless `NO_PROXY` names the address.

`verified-by: bravebot_config::import::a_checkouts_opencode_json_is_never_read`
`verified-by: bravebot_config::import::claude_config_dir_moves_where_claude_code_is_read`
`verified-by: bravebot_config::import::xdg_config_home_moves_where_opencode_is_read`
`verified-by: bravebot_config::import::ollama_host_moves_where_ollama_is_asked`
`verified-by: bravebot_config::import::an_ollama_host_off_this_machine_is_left_and_not_asked`
`verified-by: bravebot_config::import::the_lm_studio_host_moves_where_lm_studio_is_asked`
`verified-by: bravebot_config::import::llama_args_move_where_llama_server_is_asked`
`verified-by: bravebot_config::import::a_server_address_off_this_machine_is_left_and_not_asked`
`verified-by: bravebot_config::import::nothing_exported_offers_nothing_from_the_environment`

<a id="IMPORT-3"></a>
### IMPORT-3: what is offered is what bravebot's own reading keeps, and of that only what speaks its protocol

| Source | Found | Written to `~/.bravebot/settings.json` |
|---|---|---|
| Claude Code | `env.CLAUDE_CODE_USE_BEDROCK` set on, in the settings file or the process environment | `env.BRAVEBOT_USE_BEDROCK: "1"` |
| Claude Code | `env.AWS_REGION` or `env.AWS_PROFILE` in the settings file | the same names under `env` |
| Claude Code | `env.ANTHROPIC_DEFAULT_{OPUS,SONNET,HAIKU}_MODEL` in the settings file | the same names under `env` |
| Claude Code | `env.ANTHROPIC_MODEL` or `model` is `opus`, `sonnet` or `haiku`, and that tier names a model | `model` |
| Claude Code | `env.ANTHROPIC_API_KEY` in the settings file, with no `ANTHROPIC_BASE_URL` stated beside it or exported | `provider.anthropic`, at the endpoint compiled in, with the credential as [IMPORT-6](#IMPORT-6) says |
| Claude Code | `apiKeyHelper` in the settings file, where none of the cases of the next paragraph holds | `provider.anthropic`, at the endpoint compiled in: `{"env": ["ANTHROPIC_API_KEY"]}` |
| Claude Code | `env.ANTHROPIC_MODEL` or `model` is a name of the form `claude-…`, beside the `anthropic` entry this import writes | `model: "anthropic/<name>"` |
| opencode | a `provider` entry bravebot's own reading keeps, whose `npm` is absent, `@ai-sdk/openai-compatible`, `@openrouter/ai-sdk-provider` for `openrouter`, `@ai-sdk/amazon-bedrock` for `amazon-bedrock`, `@ai-sdk/google-vertex` for `google-vertex`, `@ai-sdk/anthropic` for `anthropic`, or `@ai-sdk/openai` for `openai` where it states no `baseURL` or OpenAI's | that entry under `provider`, holding only the fields bravebot reads: `name`, `env`, `models`, `options.baseURL`, `options.region`, `options.profile`, `options.project`, `options.location`, and the credential as [IMPORT-6](#IMPORT-6) says |
| opencode | an `auth.json` entry of `type: "api"` for an id whose endpoint is compiled in, with no config entry for that id | a `provider` entry for that id, with the credential as [IMPORT-6](#IMPORT-6) says |
| opencode | a top-level `model` of the form `provider/model`, where the import writes that provider's entry | `model`, in the form that names the gateway; for `amazon-bedrock`, the model's own id, added to that entry's `models` |
| Environment | `OPENROUTER_API_KEY`, `OPENAI_API_KEY` or `ANTHROPIC_API_KEY` exported and not blank, with none of that service's host variables exported ([IMPORT-2](#IMPORT-2)) | `provider.<id>`, at the endpoint compiled in: `{"env": ["<variable>"]}` |
| Ollama | a server answering [IMPORT-10](#IMPORT-10)'s request with a model it can serve | `provider.ollama`: `{"name": "Ollama (local)", "options": {"baseURL": "<address>/v1"}}`, with `"contextWindow": <tokens>` added to `options` where [IMPORT-5](#IMPORT-5) says, and `model: "ollama/<name>"` for the model [IMPORT-10](#IMPORT-10) chooses |
| LM Studio | a server answering [IMPORT-12](#IMPORT-12)'s request with a language model trained for tool use | `provider.lmstudio`: `{"name": "LM Studio (local)", "options": {"baseURL": "<address>/v1"}}`, with `"contextWindow": <tokens>` added to `options` where [IMPORT-5](#IMPORT-5) says, and `model: "lmstudio/<key>"` for the model [IMPORT-12](#IMPORT-12) chooses |
| llama-server | a server answering [IMPORT-12](#IMPORT-12)'s request with a listing it owns | `provider.llama-server`: `{"name": "llama-server (local)", "options": {"baseURL": "<address>/v1"}}`, and `model: "llama-server/<id>"` for the entry [IMPORT-12](#IMPORT-12) chooses |

**A host must name somewhere.** An entry whose host is not an `http` or `https` URL, or still holds
one of opencode's `{env:...}` substitutions, is not offered, because this program makes no
substitution and would send its requests nowhere. It is listed as left ([IMPORT-4](#IMPORT-4)).

**An entry with no `npm` is read as opencode reads it.** opencode serves `azure`, `cohere`, `google`,
`google-vertex` and `google-vertex-anthropic` through their own SDKs when the entry names none, and
`anthropic` through the Anthropic one. An entry for one of the first five is left as naming another
SDK ([IMPORT-4](#IMPORT-4)). Any other id with no `npm` is OpenAI-compatible, except `anthropic`.

**An `anthropic` entry is reached through the Messages API** ([BACKEND-57](backends.md#BACKEND-57)),
whether it names no `npm` or `@ai-sdk/anthropic`, and is offered at `https://api.anthropic.com/v1`
where it states no `baseURL`. An entry that names `@ai-sdk/anthropic` under any other id is left as
naming another SDK, since only the `anthropic` id is reached in that protocol, and so is an
`anthropic` entry that names `@ai-sdk/openai-compatible`, which is a gateway under that name and
would be read as Anthropic's once written without the field. An entry that names no
variable is written with `ANTHROPIC_API_KEY`, the variable opencode reads for this id, and an
`auth.json` key of `type: "api"` for the id is offered as an entry of its own. An `oauth` sign-in for
the id is still left ([IMPORT-4](#IMPORT-4)). The key Claude Code states as `env.ANTHROPIC_API_KEY` is
offered the same way, as an entry of its own at that endpoint, where the settings file states no
`env.ANTHROPIC_BASE_URL`. A key stated beside a host was issued for that host, and an entry written
from it would send it to Anthropic's, so the key and the host are both left
([IMPORT-4](#IMPORT-4)).

**A key command is not run.** A person who signs Claude Code in to Anthropic's API with a command
writes it as `apiKeyHelper`, and the key it prints is for that API unless the settings send it
elsewhere. So where `apiKeyHelper` is stated and nothing below holds, the import offers an `anthropic`
entry naming `ANTHROPIC_API_KEY`, which is the only place an `anthropic` entry reads a key from
([BACKEND-57](backends.md#BACKEND-57)). Whether the command is stated is the only thing read about
it: the command is not run, not copied, not shown, and nothing it holds reaches an entry, a line or a
model. The credential line shown beside the entry
says its key comes from a command that is never run, and to export what it prints as that variable
([IMPORT-6](#IMPORT-6)). The entry is offered only where none of these holds:

- a key stated as `env.ANTHROPIC_API_KEY`, which is the entry above and needs no second one;
- `CLAUDE_CODE_USE_BEDROCK`, `CLAUDE_CODE_USE_VERTEX` or `CLAUDE_CODE_USE_FOUNDRY` on, stated or
  exported, because Claude Code then reaches that account and does not use the command for
  Anthropic's. Nothing is offered or said for the command;
- `env.ANTHROPIC_BASE_URL` or `env.ANTHROPIC_AUTH_TOKEN` stated, or `ANTHROPIC_BASE_URL` exported,
  because the key the command prints was issued for that host and an entry for Anthropic's would
  send it there. The command is listed as left for that reason.

**A model Claude Code states for Anthropic's API is a name or a word.** A `claude-` followed by
letters, digits, `.`, `_` and `-` is the name Anthropic's API knows, and is written as
`anthropic/<name>` beside an `anthropic` entry this import writes ([IMPORT-5](#IMPORT-5)). Anything
else is a word Claude Code resolves itself (`sonnet`, `opusplan`, `opus[1m]`) and is listed as left,
because written as it stands a tier word names a model on Brave's roster
([`Tier::brave_model`](../../crates/config/src/bedrock.rs)) and not Anthropic's. No table of what a
word means is held, since it would go stale with each release.

**An `openai` entry is offered when it names no `npm`**, at `https://api.openai.com/v1`, where it is
reached in the Responses protocol ([BACKEND-17](backends.md#BACKEND-17)). One naming `@ai-sdk/openai`, the
package that speaks that protocol ([BACKEND-61](backends.md#BACKEND-61)), is offered the same way where it
states no `baseURL` or states OpenAI's, since the entry import writes names no SDK and is reached in that
protocol there. One that states another host is left as another SDK ([IMPORT-4](#IMPORT-4)), since the
entry written would be sent chat completions there. An entry that names no variable is written with
`OPENAI_API_KEY`, the variable opencode reads for this id.

**A `google-vertex` entry is the exception.** One naming a project is reached through the service's
OpenAI-compatible endpoint ([BACKEND-49](backends.md#BACKEND-49)), so it is offered where it states
`options.project`. One naming no project and an `options.apiKey` is Vertex AI express mode
([BACKEND-58](backends.md#BACKEND-58)), which opencode's SDK reaches with the key alone, and is offered
at the host compiled in. So is an `auth.json` key for the id where no entry states a project. One
stating neither a project nor a key is left ([IMPORT-4](#IMPORT-4)), since it relies on Google Cloud
credentials this program does not read and nothing here guesses a project, and so is one stating a
`baseURL` and no project, because opencode serves that through its own SDK and the host it states
is not one this program knows how to address. The key is not copied from the environment: an entry
naming no variable is written with `GOOGLE_API_KEY`, the variable opencode reads for this id.

**A model's `limit` is copied where bravebot reads it.** That is where both `context` and `output`
are present and at least one of them is above zero, so the written entry states the same window and
ceiling the source did, and a `limit` bravebot passes over is not written.

**The model goes with its entry.** A top-level `model` is written only beside the entry this import
writes for its provider. Where that entry is left as it is, because the settings file already has one
or `managed.json` pins the `provider` block, the model is not written: it was chosen against
opencode's entry, and a different one would answer it. The line naming the kept or pinned entry is
the one said.

**An exported key is offered for each service whose endpoint is compiled in.** The services are the
ones a session reaches by name ([BACKEND-17](backends.md#BACKEND-17)), so one added there is offered
with no change here, under the variable that table names for it. The entry holds `env` and nothing
else: a key is never read into it, and no model is written, since the key names none.
`GOOGLE_API_KEY` is not offered, because exported alone it already configures Vertex AI
([BACKEND-50](backends.md#BACKEND-50)) and a start with it is not refused. An `anthropic` entry
reads an exported key only where it names the variable ([BACKEND-57](backends.md#BACKEND-57)), so
the entry shown before the question, which holds the variable and the host the key is sent to, is
where the person writes that name down.

**A running Ollama is written as the configuration guide's block.** It has no `env`, no `apiKey`
and no `models`, so bravebot reads it as needing no credential and asks it for its roster
([backends.md](backends.md)). A model pulled after the import is then offered with nothing written
again. A running LM Studio or llama-server is written the same way, under its own id.

**Bedrock needs a region.** It is offered only where a region is named somewhere bravebot will read
it, the settings file or the process environment. Without one, bravebot treats Bedrock as not
configured, so the switch is listed as left ([IMPORT-4](#IMPORT-4)).

**The environment is not copied.** A name already set in the process environment is read from
there at run time, so it is not written to the file.

**opencode's own lists apply.** An entry in `disabled_providers` is not offered. Where
`enabled_providers` is present, only the entries it lists are considered.

**Why.** Checking against what bravebot's own reading of a `provider` block keeps gives one
definition of "supported", so a gateway added to the compiled-in endpoints widens the import with no
change here. The `npm` check is on top, because that reading ignores the field: an
`@ai-sdk/anthropic` entry under another id, with a `baseURL`, would be kept and then sent requests in
a protocol that id is not reached by. Only the fields bravebot reads are written, so the question shows everything that
is written, and an unknown field copied today cannot come to mean something unapproved once a later
build reads it.

`verified-by: bravebot_config::import::a_claude_code_bedrock_block_becomes_bravebots_own_names`
`verified-by: bravebot_config::import::bedrock_without_a_region_is_left_and_said`
`verified-by: bravebot_config::import::an_opencode_block_is_offered_exactly_when_bravebot_would_keep_it`
`verified-by: bravebot_config::import::an_entry_naming_another_sdk_is_not_offered`
`verified-by: bravebot_config::import::disabled_providers_are_not_offered`
`verified-by: bravebot_config::import::an_auth_json_key_for_a_known_id_is_a_provider`
`verified-by: bravebot_config::import::an_openai_entry_naming_no_sdk_is_offered_at_openais_endpoint`
`verified-by: bravebot_config::import::an_openai_entry_naming_the_responses_sdk_at_openais_host_is_offered`
`verified-by: bravebot_config::import::an_entry_naming_the_responses_sdk_where_the_written_entry_would_not_speak_it_is_left`
`verified-by: bravebot_config::import::an_anthropic_entry_naming_no_sdk_is_offered_at_anthropics_endpoint`
`verified-by: bravebot_config::import::an_anthropic_entry_naming_its_own_sdk_keeps_the_endpoint_it_states`
`verified-by: bravebot_config::import::an_anthropic_entry_naming_the_openai_compatible_sdk_is_left_as_another_sdk`
`verified-by: bravebot_config::import::an_anthropic_sdk_entry_under_another_id_is_left_as_another_sdk`
`verified-by: bravebot_config::import::an_auth_json_key_for_anthropic_is_a_provider_and_its_sign_in_is_not`
`verified-by: bravebot_config::import::a_claude_code_anthropic_key_is_an_anthropic_provider`
`verified-by: bravebot_config::import::a_claude_code_anthropic_key_for_another_host_or_a_bearer_token_is_left`
`verified-by: bravebot_config::import::a_key_command_is_an_anthropic_entry_naming_the_variable`
`verified-by: bravebot_config::import::a_key_command_beside_a_cloud_account_or_a_host_is_not_an_anthropic_entry`
`verified-by: bravebot_config::import::a_key_beside_an_exported_host_is_left`
`verified-by: bravebot_config::import::a_key_command_beside_a_stated_key_adds_no_second_entry`
`verified-by: bravebot_config::import::a_blank_key_command_is_not_a_setup`
`verified-by: bravebot_config::import::a_model_for_anthropics_api_is_written_only_where_it_is_a_name_and_beside_its_entry`
`verified-by: bravebot_cli::import::a_claude_code_key_command_is_written_as_an_entry_naming_the_variable`
`verified-by: bravebot_cli::import::a_claude_code_model_for_anthropics_api_is_written_beside_its_entry`
`verified-by: bravebot_cli::import::a_claude_code_anthropic_key_is_asked_about_and_written_as_an_entry`
`verified-by: bravebot_config::import::an_auth_json_key_for_openai_is_a_provider`
`verified-by: bravebot_config::import::only_the_fields_bravebot_reads_are_written`
`verified-by: bravebot_config::import::an_exported_key_is_a_provider_naming_the_variable`
`verified-by: bravebot_config::import::every_known_service_is_offered_under_the_variable_its_table_row_names`
`verified-by: bravebot_config::import::a_blank_variable_and_a_variable_that_is_no_known_services_are_not_offered`
`verified-by: bravebot_cli::import::an_exported_key_is_asked_about_and_written_as_an_entry_naming_the_variable`
`verified-by: bravebot_config::import::a_top_level_model_is_copied_where_its_provider_is`
`verified-by: bravebot_config::import::a_limit_is_written_wherever_bravebot_reads_half_of_it`
`verified-by: bravebot_config::import::a_running_ollama_is_offered_as_the_documented_block`
`verified-by: bravebot_config::import::a_running_lm_studio_is_offered_as_a_gateway_needing_no_key`
`verified-by: bravebot_config::import::a_running_llama_server_is_offered_as_a_gateway_needing_no_key`
`verified-by: bravebot_cli::import::an_opencode_model_is_written_only_beside_its_entry`
`verified-by: bravebot_cli::import::approving_llama_server_writes_its_entry_and_the_model_it_names`

<a id="IMPORT-4"></a>
### IMPORT-4: commands, permissions and other agents' sign-ins are never imported, and what was left behind is said

**Never imported.** These keys are never read into a setting:

- Claude Code: `awsAuthRefresh`, `awsCredentialExport`, `permissions`, `hooks`, `mcpServers`, and
  the value of `apiKeyHelper`. Whether `apiKeyHelper` is stated is read ([IMPORT-3](#IMPORT-3)).
- opencode: `permission`, `mcp`, `agent`, `command`, `plugin`.

**Found but not importable.** These are listed after what is importable, one line each with the
reason:

- Claude Code's `ANTHROPIC_AUTH_TOKEN` or `ANTHROPIC_BASE_URL`, its `ANTHROPIC_API_KEY` where
  `ANTHROPIC_BASE_URL` is stated beside it or exported, and its `apiKeyHelper` where
  `ANTHROPIC_BASE_URL` or `ANTHROPIC_AUTH_TOKEN` is stated or `ANTHROPIC_BASE_URL` is exported:
  settings for Anthropic's own API, which an `anthropic` entry reaches
  ([BACKEND-57](backends.md#BACKEND-57)). A bearer token is not the header that API reads a key from,
  and a host is written without the route this program appends, so the line says to add an entry;
- Claude Code's `model` or `ANTHROPIC_MODEL` where it is a word of Claude Code's own, beside an
  `anthropic` entry ([IMPORT-3](#IMPORT-3));
- `CLAUDE_CODE_USE_VERTEX`, which serves Claude models through Google Cloud credentials, and an
  opencode `google-vertex` entry naming no project and no key, or a `baseURL` and no project, which
  this program has nothing to reach the service with ([IMPORT-3](#IMPORT-3));
- a key exported beside a host variable of its own service ([IMPORT-2](#IMPORT-2)): it was issued for
  that host, and an entry at the endpoint compiled in would send it elsewhere, so the line says to add
  a `provider` block that states the host and names the variable;
- `AWS_BEARER_TOKEN_BEDROCK`, and an opencode `auth.json` key for `amazon-bedrock`, which is the
  same bearer token, since bravebot signs Bedrock requests through the AWS credential chain;
- Bedrock switched on with no region;
- opencode `oauth` and `wellknown` sign-ins;
- an entry naming another SDK;
- an id with no known endpoint and no usable `baseURL`;
- a key built from an opencode substitution inside a longer value ([IMPORT-6](#IMPORT-6));
- an `OLLAMA_HOST`, `BRAVEBOT_LM_STUDIO_HOST` or `LLAMA_ARG_HOST` naming an address other than this
  machine's loopback, which is not asked ([IMPORT-2](#IMPORT-2));
- a running Ollama that reports which of its models can call tools and names none, or that has
  pulled no model ([IMPORT-10](#IMPORT-10)), and a running LM Studio that lists no language model
  trained for tool use ([IMPORT-12](#IMPORT-12)).

Only the name and the reason are said, never a value. Where a source is found and nothing in it can
be imported, no question is asked, and these lines are said before the refusal's routes.

**Why.** A settings file may name a destination, never a permission or a command
([backends.md](backends.md)). An import writes a settings file, so it may carry no more than one
could hold. The lines about what was left are there because a person who uses Claude Code with a
bearer token or a host of their own would otherwise see the routes and no sign that their
setup was looked at.

`verified-by: bravebot_config::import::a_google_vertex_entry_naming_a_project_is_offered_with_its_project_and_location`
`verified-by: bravebot_config::import::a_google_vertex_entry_naming_a_key_and_no_project_is_offered_in_express_mode`
`verified-by: bravebot_config::import::an_auth_json_key_for_google_vertex_is_offered_in_express_mode`
`verified-by: bravebot_config::import::a_google_vertex_entry_naming_no_project_and_no_key_is_left_and_said`
`verified-by: bravebot_config::import::a_google_vertex_entry_naming_a_base_url_and_no_project_is_left_and_said`
`verified-by: bravebot_config::import::an_entry_naming_the_responses_sdk_where_the_written_entry_would_not_speak_it_is_left`
`verified-by: bravebot_config::import::an_api_key_helper_is_never_imported`
`verified-by: bravebot_config::import::permissions_hooks_and_mcp_servers_are_never_imported`
`verified-by: bravebot_config::import::what_claude_code_uses_and_bravebot_cannot_is_named_and_not_shown`
`verified-by: bravebot_config::import::an_ollama_with_no_model_that_can_call_tools_is_left_and_said`
`verified-by: bravebot_config::import::an_lm_studio_with_no_model_trained_for_tool_use_is_left_and_said`
`verified-by: bravebot_cli::running::what_was_found_and_left_is_said_before_the_routes`
`verified-by: bravebot_cli::running::a_claude_code_anthropic_key_is_importable_and_the_refusal_names_the_command`
`verified-by: bravebot_config::import::a_key_exported_beside_a_host_of_its_own_is_left`
`verified-by: bravebot_cli::import::a_key_exported_beside_a_host_is_said_to_be_left_and_not_asked_about`
`verified-by: bravebot_cli::running::a_key_exported_beside_a_host_is_said_to_be_left_before_the_routes`
`verified-by: bravebot_cli::running::a_claude_code_key_command_is_importable_and_the_refusal_names_the_command`

<a id="IMPORT-5"></a>
### IMPORT-5: everything that would be written is shown before the question, and only the affirmative writes

**One question per source.** Claude Code is asked about first, then opencode, then the keys exported
in the environment, then Ollama, LM Studio and llama-server.

**What is shown before each question:**

- the files it was read from, and the file that will be written. A setup found only in the process
  environment says it was found there rather than naming no file, and a running server names the
  address that answered;
- every name it would add, with its value verbatim: each host, region, profile, variable name and
  model id;
- the credential lines, as [IMPORT-6](#IMPORT-6) says.

**What counts as an answer.** Only the affirmative approves. Any other line declines, and so does
the end of input.

**The window a running server holds.** Under the model line of an Ollama or LM Studio question,
where [IMPORT-10](#IMPORT-10) or [IMPORT-12](#IMPORT-12) read a figure for that model, the question
gives the context window the server has it loaded with. Where the figure is below the window a
conversation is otherwise budgeted against (131,072 tokens), an Ollama question also says the
server drops the earliest turns beyond it and names `OLLAMA_CONTEXT_LENGTH`, and an LM Studio
question says a longer conversation does not fit and that the model can be loaded with a longer
context length in LM Studio. A model with no figure has no line, and no figure is guessed.

**Where the window is kept.** A figure below that window is written as `options.contextWindow` of
the entry, shown in the entry the question lists, and a session budgets the entry's models against
it ([backends.md](backends.md)). It is a field of the provider because the entry lists no models to
hold a `limit`, and a block that lists models is taken at its word. A figure at or above the window
is not written, since the window a conversation is otherwise budgeted against is the safe
assumption and a model reloaded larger would stay held down by a stale figure.

**Names already set.** A name already set in the file, including one the first answer just wrote,
is shown as left as it is ([IMPORT-7](#IMPORT-7)). A source left with nothing to add is not asked
about.

**A model a tier names.** A model an `amazon-bedrock` entry lists is left out of it where a tier
variable names that model, read the way a start will read it once the file is written: pinned,
exported, or in the file, including what the first answer wrote. It is shown under its own line,
with the variable that names it. An entry left listing no model is not written.

**Why.** A prompt shows what is at stake rather than a summary of it ([prompting.md](prompting.md)).
The host is where a credential will be sent, and a host is something a person can approve on sight,
shown exactly as it will be written. One question per source, rather than one per item, keeps a
Bedrock setup from being half imported; one per source rather than one for both lets the person
choose when the two disagree. The tiers answer for a model before any entry does, so a copy in the
entry would be a second row in the model picker that no request is ever sent through, and an entry
listing only such models reaches nothing.

`verified-by: bravebot_cli::import::every_host_and_name_written_is_shown_before_the_question`
`verified-by: bravebot_cli::import::only_the_affirmative_writes_and_the_end_of_input_declines`
`verified-by: bravebot_cli::import::a_declined_exported_key_writes_nothing`
`verified-by: bravebot_cli::import::a_name_the_first_answer_wrote_is_left_by_the_second`
`verified-by: bravebot_cli::import::a_setup_found_only_in_the_environment_says_so`
`verified-by: bravebot_cli::import::a_window_below_the_assumed_one_is_shown_with_what_the_server_drops`
`verified-by: bravebot_cli::import::a_small_window_is_shown_for_lm_studio_in_its_own_words`
`verified-by: bravebot_cli::import::each_server_is_asked_about_on_its_own`
`verified-by: bravebot_cli::import::approving_a_small_window_writes_it_into_the_file`
`verified-by: bravebot_config::import::a_window_below_the_assumed_one_is_written_into_the_entry`
`verified-by: bravebot_config::import::an_lm_studios_window_below_the_assumed_one_is_written_into_the_entry`
`verified-by: bravebot_config::provider::a_provider_window_is_the_default_for_models_that_state_none`
`verified-by: bravebot_aichat::models::a_provider_window_is_taken_for_fetched_models`
`verified-by: bravebot_cli::import::a_model_a_tier_names_is_not_added_to_an_aws_entry_again`
`verified-by: bravebot_cli::import::an_exported_tier_names_a_model_as_a_written_one_does`

<a id="IMPORT-6"></a>
### IMPORT-6: a credential value is written only on its own question, and is never shown

**Where the source names a variable, only the name is written:** Claude Code's `env` names,
opencode's `env` array, and opencode's `{env:VAR}` in `options.apiKey`, which becomes
`env: ["VAR"]` rather than the literal string.

**Where the source holds the value itself** (a literal `options.apiKey`, an `auth.json`
`type: "api"` key, or Claude Code's `env.ANTHROPIC_API_KEY`), the source's question does not write it. Once that question is approved, a
second question follows, one per value. It names the entry, the host the key is sent to and the
file it would be kept in, says the key is kept there in plain text, and does not show the key.

An affirmative answer writes `options.apiKey`. Any other answer writes the entry with `env` naming
the variable opencode itself reads for that id, compiled in beside the endpoint (`openrouter` reads
`OPENROUTER_API_KEY` and `openai` reads `OPENAI_API_KEY`), and says to export it. Where no variable
is known, the entry is written with no credential, which is what a local model server needs, and the
line says so.

A `{file:path}` credential is not followed. It is treated as a declined key, and its line names the
path.

**Where the source runs a command for the key** (Claude Code's `apiKeyHelper`, [IMPORT-3](#IMPORT-3)),
no value is read and no question about a key is asked. The entry names `ANTHROPIC_API_KEY`, and the
credential line says that the key comes from a command, which is never run, and to export what it
prints as that variable. The line names neither the command nor anything in it.

A substitution is read only where it is the whole value. A key such as `Bearer {env:TOKEN}`, two
substitutions side by side, or an `{env:}` naming no variable would need bravebot to make the
substitution opencode makes, and written as it stands it is not the key opencode sends. That entry is
left ([IMPORT-4](#IMPORT-4)), and its line never shows the value.

A value that is read is held as a secret and cleared when it goes, and is never logged, traced or
shown.

**Why.** Keeping a turn from copying a credential somewhere weaker says nothing about the person
([credential-protection.md](credential-protection.md)), which is why this is a question rather than
a refusal. The default is still the variable: a named variable is how a gateway credential is
preferred ([backends.md](backends.md)), and a plaintext key in the settings file is one of that
spec's known costs, which an import should not make the easy path.

`verified-by: bravebot_config::import::an_env_reference_becomes_a_variable_name_not_a_token`
`verified-by: bravebot_config::import::a_literal_key_is_held_apart_from_the_entry`
`verified-by: bravebot_config::import::a_file_reference_is_not_followed`
`verified-by: bravebot_config::import::a_key_built_from_a_substitution_is_left`
`verified-by: bravebot_cli::import::a_held_key_is_asked_about_separately_and_never_drawn`
`verified-by: bravebot_cli::import::a_declined_key_names_the_variable_to_export`
`verified-by: bravebot_cli::import::a_file_key_is_shown_by_path_and_treated_as_declined`

<a id="IMPORT-7"></a>
### IMPORT-7: the import writes the user layer only, and replaces no value bravebot reads

It writes `~/.bravebot/settings.json` and no other layer.

**Existing names.** A name set there as bravebot reads it keeps its value and is shown as left as it
is: an `env` name holding a string, a `model` holding a word, and any `provider` entry. A value
bravebot's reading passes over, such as a number under `env` or a blank `model`, configures nothing,
so the import replaces it, and the question shows the new value as it shows any other.

**A file that does not parse** is not written. The import says so and writes nothing, because
rewriting it would lose what the person wrote. The same holds for a file past the size bravebot
reads, or one the import would take past it, since bravebot would then read none of it.

**A file that changes while the questions are asked** is not written. The questions take as long as
the person does, and another program may write the file meanwhile. The import compares the file with
what it read before the first question, and where the two differ it says so and names
`bravebot import-providers` rather than write over what was put there.

**How it writes.** To a temporary file beside it, created readable only by the user as everything
in `~/.bravebot` is ([state-directory.md](state-directory.md)), renamed over the real file. A failed
write leaves the old file in place, and no copy of a key is readable by another user in between.

**Managed pins.** A name the machine-level `managed.json` pins is not offered, because the pinned
value would override it. The line naming it says which file pins it.

**Incognito.** Nothing is offered or written while incognito, and the refusal is the one a start
with nothing configured gets. Incognito adds nothing to `~/.bravebot`
([incognito.md](incognito.md)).

`verified-by: bravebot_config::import::a_destination_adds_names_and_replaces_none`
`verified-by: bravebot_cli::import::a_name_already_set_keeps_its_value`
`verified-by: bravebot_cli::import::an_exported_key_for_an_entry_the_file_already_has_is_left_as_it_is`
`verified-by: bravebot_cli::import::a_key_command_and_an_exported_key_for_anthropic_are_asked_about_once`
`verified-by: bravebot_cli::import::a_settings_file_that_does_not_parse_is_not_rewritten`
`verified-by: bravebot_cli::import::a_file_changed_while_asking_is_not_written_over`
`verified-by: bravebot_cli::import::the_written_file_is_readable_only_by_the_user`
`verified-by: bravebot_cli::import::a_name_managed_pins_is_not_offered`
`verified-by: bravebot_cli::running::an_incognito_first_run_offers_no_import`

<a id="IMPORT-8"></a>
### IMPORT-8: where nobody can be asked, nothing is imported, and the refusal names the command that asks

These never ask, because where nobody can be asked the answer is no ([prompting.md](prompting.md)):
a one-shot run, `--json`, a stdin, stdout or stderr that is not a terminal, and `doctor`.

In those cases, if [IMPORT-2](#IMPORT-2) finds something importable, the refusal gains one line
naming every source that holds something and `bravebot auth login import`, which runs
`bravebot import-providers` ([CLI-18](cli.md#CLI-18)). `doctor` repeats the refusal, so it gets the same line. Where the
settings file is one the import would refuse to write ([IMPORT-7](#IMPORT-7)), the line names that
file and the reason instead, because the command would only refuse.

`bravebot import-providers` asks the [IMPORT-5](#IMPORT-5) questions at any time, whether or not a
service is configured. It refuses where stdin or stderr is not a terminal, and it refuses while incognito.
Where nothing is left to import it says so, along with what [IMPORT-4](#IMPORT-4) found and left.

**Why.** It is a command a person types, as the sign-ins under `auth login` are, rather than a flag
that answers yes on their behalf.

`verified-by: bravebot_cli::running::a_one_shot_first_run_names_the_import_command_and_asks_nothing`
`verified-by: bravebot_cli::running::an_exported_key_is_importable_and_the_refusal_names_the_command`
`verified-by: bravebot_cli::running::a_one_shot_first_run_with_ollama_running_names_the_import_command`
`verified-by: bravebot_cli::running::a_one_shot_first_run_with_a_server_running_names_the_import_command`
`verified-by: bravebot_cli::import::the_command_line_names_every_source_that_holds_something`
`verified-by: bravebot_cli::running::import_providers_is_refused_where_its_input_is_not_a_terminal`
`verified-by: bravebot_cli::running::import_providers_is_refused_while_incognito`
`verified-by: bravebot_cli::running::nothing_is_asked_where_stderr_is_not_a_terminal`
`verified-by: bravebot_cli::running::a_settings_file_the_import_cannot_write_is_named_in_place_of_the_command`
`verified-by: bravebot_cli::running::a_settings_file_the_import_would_take_too_large_is_named_in_place_of_the_command`

<a id="IMPORT-9"></a>
### IMPORT-9: after a write, the start reads its settings again and opens the session if a service now answers

After an approved write, the configuration is read again from disk, exactly as a fresh start reads
it, every settings layer and `managed.json` included. Then the decision whether to refuse is taken
again.

- If the model the session would run on is served by a written entry whose variable is not set in
  this environment, the start ends before the session opens and names the variable to export.
- If a service now answers, the session opens, after one line saying what was imported and where
  from. A written entry that serves another model and whose variable is not set gets a line naming
  the variable, and does not stop the session: its models answer once the variable is exported,
  since a gateway's credential is read when a request needs it ([backends.md](backends.md)).
- Otherwise the refusal for that case is shown. Where the source named no default model, that is
  the one naming the `model` key.

**Why.** It re-reads rather than using what it just built, because the file on disk is what every
later start reads. The first session then runs on the same configuration as the ones after it, and
a write that did not produce a working configuration is found now rather than next time. Only the
session's own model decides whether it ends: an entry the session does not use cannot stop it
working, and ending over one would refuse a service that answers.

`verified-by: bravebot_cli::running::an_approved_import_opens_the_session_on_the_file_it_wrote`
`verified-by: bravebot_cli::running::an_unset_variable_after_an_import_is_named_before_the_session_opens`
`verified-by: bravebot_cli::running::an_unset_variable_for_another_entry_is_said_and_the_session_opens`
`verified-by: bravebot_cli::running::an_import_that_leaves_no_model_to_run_ends_with_the_refusal_for_that_case`

<a id="IMPORT-10"></a>
### IMPORT-10: a running Ollama is asked on this machine, within a bound, and says nothing where it does not answer

**When.** Ollama is asked wherever the other two sources are read: at the start
[IMPORT-1](#IMPORT-1) describes, for [IMPORT-8](#IMPORT-8)'s refusal lines, and by
`bravebot import-providers`. It is asked after the other two, and only once there is a settings
file the import could write, so an incognito start opens no connection. A start with a service
configured asks nothing. LM Studio and llama-server are asked at the same times, after Ollama
([IMPORT-12](#IMPORT-12)).

**What is asked.** One `GET <address>/api/tags`, and then the one `GET <address>/api/ps` of the window below, each to the [IMPORT-2](#IMPORT-2) address, carrying no
credential. A redirect is followed only where it keeps that address and port, so whatever
listens there cannot send the request off the machine. It is bounded twice, because it is made
before the start has said anything:

- 1 second each to resolve, to connect and to send, and 2 seconds for the reply to begin and 2
  more for its body, so a listener that accepts and never answers holds the start for seconds;
- a listing over 1 MiB is not read.

Only each model's `name`, `modified_at` and `capabilities` are read.

**The window.** Once a listing is read, one `GET <address>/api/ps` is made under the same bounds
and carries no credential, and each loaded model's `context_length` is read by `name`. A model that
is not loaded, a zero figure, and a reply that fails or does not decode leave that model with no
figure, and nothing is said. A server that answers the listing and then holds this request
holds the start for those bounds a second time. The figure only decides what [IMPORT-5](#IMPORT-5) shows beside the
model, before any session exists, and the reply is read by no model.

**A failure says nothing.** A refused connection, a timeout, an error status, a redirect
elsewhere, a listing over the bound, or one that does not decode means there is no Ollama source, and nothing is said
about it, because nothing on the machine said an Ollama was meant to be there.

**The model.** The one written is the listed model with the latest `modified_at`, compared as
instants rather than taken from the listing's order, among those whose `capabilities` include
`tools`. Where no model reports capabilities, it is the latest of them all. A model with no name
is never chosen, and a time that does not read counts as the oldest. Where the server reports
capabilities and none lists `tools`, or it has pulled nothing, the source is left
([IMPORT-4](#IMPORT-4)).

**Not offered twice.** The Ollama entry is left as it is, whatever the other entry's id, where the
settings file has any `provider` entry whose `options.baseURL` is an `http` or `https` address on
this machine at the same port. That includes an entry an earlier answer in the same run wrote.
The line naming that entry is the one said ([IMPORT-7](#IMPORT-7)). A `provider.ollama` naming
another address is kept as [IMPORT-7](#IMPORT-7) says, and the running server is then not offered.
The same holds for LM Studio and llama-server ([IMPORT-12](#IMPORT-12)).

**Why.** A person who runs Ollama and nothing else would otherwise copy the configuration guide's
block by hand. The request is `/api/tags` rather than the `/v1/models` roster the written block is
later asked, because only it says which models can call tools. Every turn here calls tools, so a
model that reports only completion would fail its first turn. Two entries for one server would
list every model twice in the model picker.

`verified-by: bravebot_config::import::the_default_model_is_the_newest_that_can_call_tools`
`verified-by: bravebot_config::import::an_ollama_reporting_no_capabilities_defaults_to_its_newest_model`
`verified-by: bravebot_config::import::an_ollama_that_does_not_answer_is_no_source`
`verified-by: bravebot_aichat::ollama::a_listing_is_read_as_ollama_writes_it`
`verified-by: bravebot_aichat::ollama::a_loaded_model_carries_the_window_it_runs_with`
`verified-by: bravebot_aichat::ollama::a_failed_question_about_what_is_loaded_leaves_the_listing_without_windows`
`verified-by: bravebot_config::import::the_window_is_the_one_of_the_model_chosen`
`verified-by: bravebot_aichat::ollama::an_oversized_listing_is_not_read`
`verified-by: bravebot_aichat::ollama::a_listener_that_never_answers_does_not_hold_the_start`
`verified-by: bravebot_aichat::ollama::anything_but_a_listing_is_no_source`
`verified-by: bravebot_aichat::ollama::a_redirect_off_the_address_asked_is_not_followed`
`verified-by: bravebot_cli::import::an_ollama_already_configured_under_another_id_is_not_offered_again`
`verified-by: bravebot_cli::import::an_opencode_entry_for_the_same_server_leaves_the_ollama_source_with_nothing_to_add`
`verified-by: bravebot_cli::running::a_first_run_with_nothing_listening_refuses_as_before`
`verified-by: bravebot_cli::running::a_first_run_whose_settings_file_cannot_be_imported_into_asks_ollama_one_round_only`
`verified-by: bravebot_cli::running::a_start_with_no_service_asks_the_ollama_on_this_machine_for_its_listing_and_what_is_loaded`
`verified-by: bravebot_cli::running::an_incognito_start_opens_no_connection_to_ollama`
`verified-by: bravebot_cli::running::a_start_with_a_service_configured_asks_ollama_nothing`

<a id="IMPORT-11"></a>
### IMPORT-11: a session is copied only when a person runs the command, and only as words to read

`bravebot sessions import claude-code` reads the transcripts Claude Code keeps under its directory
([IMPORT-2](#IMPORT-2) names which: `$CLAUDE_CONFIG_DIR`, or `~/.claude`) for one workspace.
[SESSION-32](#SESSION-32) governs what it takes and what the copy holds. Here is the boundary
with the rest of the import:

- It runs when a person types it, and at no start, no resume and no turn. Nothing offers it.
- It reads that one program's transcript files for that workspace and nothing else in the
  directory: not the settings, not `.credentials.json`.
- It is a one-time copy. After it, the other program's files are not read again, so a session
  edited or deleted there is not followed.
- It changes no configuration, permission, trust or route.

`bravebot sessions import opencode` is refused with a line saying its sessions are not readable.

**Why.** A person leaving a tool keeps the history they wanted to refer to, and a copy that
followed the other tool would be a fallback to another program's data under another name.

`verified-by: bravebot_cli::running::a_claude_code_session_is_copied_once_and_only_when_asked`
`verified-by: bravebot_session::import::a_second_copy_changes_nothing`

<a id="IMPORT-12"></a>
### IMPORT-12: a running LM Studio or llama-server is asked as Ollama is, at its own path

**When.** Both are asked wherever Ollama is ([IMPORT-10](#IMPORT-10)): after it, LM Studio first
and llama-server second, and only once there is a settings file the import could write. A start
with a service configured asks neither, and an incognito start opens no connection to either.

**Together.** Every address of the three servers is asked at the same time, so a start where
several of them accept a connection and never answer waits for one bound, not for the sum of them.

**What is asked.** One `GET` each, to the [IMPORT-2](#IMPORT-2) address, carrying no credential and
under [IMPORT-10](#IMPORT-10)'s bounds: a redirect is followed only where it keeps the address and
port, each request has the same 1 second and 2 second limits, and a listing over 1 MiB is not read.

- LM Studio: `GET <address>/api/v1/models`. Only each model's `type`, `key`,
  `capabilities.trained_for_tool_use` and the `context_length` of each of its `loaded_instances` are
  read. Only a model of type `llm` is offered, since the listing names embedding models beside the
  language models.
- llama-server: `GET <address>/v1/models`. Only each entry's `id` and `owned_by` are read, and only an
  entry that carries an `id` and whose `owned_by` is `llamacpp` counts. Every OpenAI-compatible server answers that path, and
  other programs listen on the ports llama-server defaults to, so a listing with no such entry is
  not llama-server's and is no source.

**A failure says nothing.** A refused connection, a timeout, an error status (a server that
requires a key for its listing answers one), a redirect elsewhere, a listing over the bound, and one that does not
decode mean there is no source, and nothing is said about it.

**The model.** LM Studio states no time, so no model is the newest. The one written is the first it
lists that is trained for tool use and loaded, meaning it has a loaded instance that states a
context length, or the first it lists that is trained for tool use where none of those is loaded. A model with no name is never chosen. Where LM Studio lists no
language model trained for tool use, or none at all, the source is left
([IMPORT-4](#IMPORT-4)). llama-server reports no capabilities, so none is checked: the one written
is the first entry it owns.

**The window.** LM Studio's figure for the model written is the smallest `context_length` above
zero among that model's loaded instances, since a request may reach any of them, and it is shown
and written as [IMPORT-5](#IMPORT-5) says. llama-server's listing states the context the model was
trained for and not the one the server was started with, so no figure is read for it.

**Where llama-server is asked.** At the port `LLAMA_ARG_PORT` names, where it names one. Where it
names none, at `9931` and at `8080`, and the first of them that answers with a listing it owns is
offered, `9931` where both do.

**Not offered twice.** [IMPORT-10](#IMPORT-10)'s rule by port holds among all three servers. They
are asked about in the order Ollama, LM Studio, llama-server, so where two share a port the earlier
is offered first, and the later is offered where the first was declined.

**Why.** A person who runs LM Studio or llama-server and nothing else would otherwise write the
block by hand, as one who runs Ollama would. They are asked the way Ollama is so that each
guarantee made for it holds for them: nothing leaves this machine before a question, the wait is
bounded, a failure is silent, and the person sees every name before it is written. The listings
are read before any session exists and reach no planner. LM Studio's says which models can call
tools and what window they hold, which a session needs, so it is read for both. llama-server's says
neither, so the entry is written and a window is assumed as it is for any gateway. The `owned_by`
check is there because llama-server's path and ports are shared with programs that are not it. The
ports are asked together because a start waits on them before it has said anything, and
llama-server has two default ports, since a release that predates the current default and its
container image listen on `8080`.

`verified-by: bravebot_aichat::lm_studio::a_listing_is_read_as_lm_studio_writes_it`
`verified-by: bravebot_aichat::lm_studio::a_model_loaded_twice_holds_the_smaller_window`
`verified-by: bravebot_aichat::lm_studio::only_the_path_lm_studio_answers_is_asked`
`verified-by: bravebot_aichat::lm_studio::anything_but_a_listing_is_no_source`
`verified-by: bravebot_aichat::llama_server::a_listing_is_read_as_llama_server_writes_it`
`verified-by: bravebot_aichat::llama_server::another_servers_listing_is_not_offered_as_llama_servers`
`verified-by: bravebot_aichat::llama_server::only_the_entries_llama_server_owns_are_listed`
`verified-by: bravebot_aichat::llama_server::anything_but_a_listing_is_no_source`
`verified-by: bravebot_aichat::local_servers::an_oversized_listing_is_read_from_none_of_them`
`verified-by: bravebot_aichat::local_servers::a_listener_that_never_answers_holds_none_of_them`
`verified-by: bravebot_aichat::local_servers::a_redirect_off_the_address_asked_is_followed_by_none_of_them`
`verified-by: bravebot_config::import::the_model_chosen_from_lm_studio_is_one_it_has_loaded_that_can_call_tools`
`verified-by: bravebot_config::import::llama_server_is_found_at_either_port_it_has_defaulted_to`
`verified-by: bravebot_config::import::every_server_is_asked_at_once`
`verified-by: bravebot_config::import::the_servers_are_offered_in_a_fixed_order`
`verified-by: bravebot_cli::import::a_server_at_a_port_an_earlier_answer_wrote_is_not_offered_again`
`verified-by: bravebot_cli::running::a_first_run_with_lm_studio_running_offers_to_import_it`
`verified-by: bravebot_cli::running::a_first_run_with_llama_server_running_offers_to_import_it`
`verified-by: bravebot_cli::running::a_first_run_with_neither_listening_says_nothing_of_either`
`verified-by: bravebot_cli::running::a_start_with_no_service_asks_lm_studio_and_llama_server_once_each`
`verified-by: bravebot_cli::running::an_incognito_start_opens_no_connection_to_lm_studio_or_llama_server`
`verified-by: bravebot_cli::running::a_start_with_a_service_configured_asks_lm_studio_and_llama_server_nothing`

## Where this stands against the rule

No model runs during an import. Nothing it reads reaches the planner, a turn, a session record or a
trace, and no session exists yet when it runs. The files it reads are the person's own
configuration, in their home directory, and every value it writes has been shown to them and
approved ([IMPORT-5](#IMPORT-5)). A checkout's files, which carry a repository author's bytes, are
never opened ([IMPORT-2](#IMPORT-2)).

The import makes up to five requests before any question: two to a running Ollama
([IMPORT-10](#IMPORT-10)), one to a running LM Studio, and one to a running llama-server at each of
its ports ([IMPORT-12](#IMPORT-12)). Each body is labelled untrusted, as any roster's is. What it
decides is whether to offer the source and which model to show. Both are decided before any session exists and reach no planner, which is
the case [reviewing-for-the-rule.md](../development/reviewing-for-the-rule.md) calls the inverse
mistake to flag. The only fetched value written to disk is a model name the person approved on
sight.

A session import runs no model either, and its words reach no request: they are stored as the
archive, which is drawn and never sent ([SESSION-32](#SESSION-32)). Putting them in a request is
the owner's decision.

## Known costs

- **An exported key may be scoped for another tool.** The import cannot tell, so it offers a key
  exported under a known variable name as it offers any other, after showing the variable and the
  host. A key exported beside its service's host variable is left. A key restricted to a project or a
  model that this program does not use is offered all the same, and its requests fail.
- **A start that imports an exported key ends in the refusal for a missing model.** The key names no
  model, so the start that wrote the entry ends as [IMPORT-9](#IMPORT-9) says, and so does every start after it until `model` is set,
  since nothing yet asks which model to use ([#3440](https://github.com/brave/bravebot/issues/3440)).

- **Two copies of the configuration.** After an import the two tools drift apart, and bravebot does
  not follow later edits to the other tool's files. The alternative, reading them at every start,
  is a fallback to another program's configuration under another name.
- **The settings file is rewritten whole.** Every value is kept, but its spacing and key order are
  not, because `serde_json` is built without `preserve_order`.
- **A plaintext key.** A key approved under [IMPORT-6](#IMPORT-6) sits in plain text in
  `~/.bravebot/settings.json`, reached only by a question.
- **Unsupported setups still get the routes,** with the [IMPORT-4](#IMPORT-4) lines first:
  a Claude Code user on a claude.ai subscription, a bearer token or a host of their own, and an
  opencode user on a provider SDK other than an OpenAI-compatible one.
- **A program that can write the other tool's files can name a host**, which the import then shows
  for approval. Such a program could equally write `~/.bravebot/settings.json`, so the import gives
  it no new reach. What the import adds is that the host is shown before anything is sent to it.
- **Any program listening at an address asked chooses the names offered.** It can answer the
  listing, and so choose the model name the question shows. It cannot choose the host, which is
  the address a variable or the default names, and the name is shown before it is written. For
  llama-server it also has to say `llamacpp`, which a program meaning to be offered can say.
- **Up to four loopback connections per refused start**, and per `doctor` or `--json` run, where no
  service is configured and a settings file could be written: one to each of Ollama and LM Studio,
  and one to each port llama-server defaults to.
- **A server that needs a key is offered without one, or not at all.** The request carries no
  credential. A server that requires one for its listing answers an error and is not offered, and
  the person writes the block by hand. One that lists its models without a key is offered with an
  entry naming no key, whose requests fail until the block names the variable.
- **A model that is not loaded states no window.** LM Studio loads it on the first request with a
  context length of its own, which may be below the window assumed for a gateway, and no
  `contextWindow` is written for it. The same holds for Ollama.
- **An LM Studio that does not serve `/api/v1/models` is not offered,** and says nothing, as for
  any server that does not answer.
- **LM Studio on another port is found only through `BRAVEBOT_LM_STUDIO_HOST`,** and llama-server
  only on its default ports or the one `LLAMA_ARG_PORT` names. A server on any other port needs the
  block written by hand.
- **llama-server's window is not asked.** Its listing carries the context the model was trained
  for, so a server started with a smaller context is written with no `contextWindow`, and a
  conversation is budgeted against the window assumed for any gateway.
- **llama-server's tool support is not checked.** Its listing reports no capabilities, so a server
  started without a chat template that supports tools is offered and fails the first tool call.
- **opencode sessions are not importable.** They are rows in a database file, and reading one
  needs a dependency nobody has agreed to.
