# token-bench

Compares the tokens a coding agent spends on the same tasks, with the same model and cache
settings. It runs bravebot, Claude Code, Codex and opencode headless, plus a minimal `reference` loop that
calls an OpenAI-compatible endpoint (OpenRouter by default) directly. The reference loop shows what
a plain agent costs on a route, so the difference between it and a tool is the tool's own overhead.

Standard library Python. Plots need matplotlib.

```sh
contrib/token-bench/bench.py --selftest
contrib/token-bench/bench.py list --config contrib/token-bench/contestants.example.json
contrib/token-bench/bench.py run --config my.json --repeats 3
contrib/token-bench/bench.py report contrib/token-bench/results/<file>.jsonl --plot plots/
```

`run` writes one JSON line per turn to `results/` (ignored by git) and prints the ranking when it
ends. `report` reads that file again, so a ranking and the plots can be remade without running
anything. `--only` and `--scenarios` narrow a run, and `--no-warmup` skips the unscored first call.

## What is measured

Each tool reports usage differently, so `bench.py` reduces every turn to five numbers:

| Number | Meaning |
|---|---|
| `fresh` | prompt tokens neither read from nor written to a cache |
| `cache_read` | prompt tokens served from the cache |
| `cache_write` | prompt tokens written to the cache |
| `output` | tokens the model wrote |
| `requests` | model calls the turn made |

The tools differ in ways that would skew the numbers if read raw. `bench.py` handles each one:

- **bravebot** `--json`: `total` is prompt plus output, so `fresh` is what is left after output
  and both cache figures.
- **Claude Code** `--output-format json`: `input_tokens` is net of the cache. The usage is already
  summed over every request in the invocation.
- **Codex** `exec --json`: `input_tokens` includes the cached part, and a resumed turn reports the
  whole thread's running total. The turn's cost is the difference from the turn before.
- **opencode** `run --format json`: the event stream leaves out the last request's usage, so the
  session is exported afterwards and each assistant message's `tokens` are summed. `input` is net of
  the cache and reasoning tokens are counted as output. A later turn's cost is the difference from
  the totals before it. The session is deleted when its scenario ends.
- **reference**: reads `prompt_tokens_details.cached_tokens` and `cache_write_tokens`, and puts a
  cache mark on the system prompt and the last message.

The selftest runs the parsers on output captured from the real tools, with totals checked by hand.

## Score

A turn's cost is one number in units of an uncached input token: fresh tokens count 1, cache
reads 0.10, cache writes 1.25 (Anthropic) or 1.0 (OpenAI), and output 5 or 8. A contestant can
choose a profile or state its own numbers with `"weights": {"write": 1.25, "read": 0.1,
"output": 5}`. These are list-price ratios, so change them if the prices you pay differ.

For each scenario, the cheapest contestant that solved it in every repeat scores 100, and the others
score `100 * best / cost`. A contestant that failed any repeat is scaled down by its success rate,
so a cheap wrong answer cannot win. A contestant's score is the mean over scenarios, and each
category (overhead, tools, cache, delegates, skills, configured, modes, edits, system_prompt) has its own mean. Costs are medians over repeats.

## Scenarios

| Scenario | Category | Measures |
|---|---|---|
| `hello` | overhead | the fixed cost of the system prompt and tool schemas |
| `read_count` | tools | a few file reads and a sum |
| `fix_bug` | tools | reading a failing script, an edit and a re-run, checked by running the test |
| `find_symbol` | tools | searching forty files for one definition |
| `multi_turn` | cache | five turns over one conversation, and how much of each is read from the cache |
| `sysprompt_short`, `sysprompt_long` | system_prompt | the effect of 10 and 150 added rules |
| `delegate_fanout` | delegates | four delegates started in one turn, each reading one file |
| `delegate_serial` | delegates | three turns over one conversation, each starting one delegate |
| `configured_hello` | configured | one trivial turn with eight skills and eight MCP tool schemas loaded, without `--safe` |
| `skill_load` | skills | a turn that calls `load_skill` for a skill of about five thousand characters, then a turn about its text |
| `skill_inline` | skills | the turns of `skill_load` with the values in the prompt and no skill loaded |
| `plain_copy` | modes | copying one file to another in a plain turn, checked on the files |
| `goal_copy` | modes | the task of `plain_copy` as a `bravebot --goal` run |
| `manifest_copy` | modes | the task of `plain_copy` as a `bravebot --mode manifest` run |
| `refactor_rename` | edits | renaming a function across four modules and a notes file, checked by running the project's test script |

The `delegates` scenarios measure what a session spends when it starts sub-agents: the delegates'
own prompts, how much of them is read from the cache, and what the parent's next request reads
after a delegate returns. For bravebot the `--json` totals include every delegate's usage, so the
figures are the whole session's. A delegate has its own conversation, so the `cache_write` and
`cache_read` columns on `delegate_fanout` show whether four delegates started together read a
prefix that is already cached or each write their own.

bravebot reports the `spawn_agent` calls a turn made, and a turn that asks for delegates fails when
fewer were started, so a contestant cannot win by doing the work inline. The other tools do not
report their sub-agents in a form `bench.py` reads, so they are judged on the reply alone and
their delegate count shows as 0. Their totals may or may not include sub-agent usage, so compare
the `delegates` category across tools only after checking that. A contestant is a tool and a model
route, so the two delegate scenarios run once per route, and the `delegates` figure of each contestant
shows what delegates cost on that route.

Every scenario has a check on the result, so a tool that spends nothing by not doing the work
fails. Add a scenario by adding a `Scenario` to `scenarios()`.

### Configuration, MCP tools and skills

The other scenarios run with `--safe`, so they leave out what a person's own setup adds to every
request. Three scenarios run with that setup instead. A scenario with a `home` function writes a
fresh state directory under the temporary directory and runs the contestant with `BRAVEBOT_HOME` set
to it, so the person's own `~/.bravebot` is never read. The directory holds skills, an `mcp.json`
declaring one stdio server, and a `settings.json` that requests it. The server is
`fixtures/stub_mcp_server.py`, a standard library program that offers eight tools. A run without
`--safe` and with permission checks off starts it without a question, so the tool schemas are in
every request.

A contestant takes part in these scenarios when it has `"configured": true`. It has to be bravebot
and must not pass `--safe`, and it runs only these scenarios and none of the ones that run with `--safe`.
`"baseline"` names the `--safe` contestant of the same model and route, and the report prints the
difference between the two:

```
configured overhead: configured_hello minus the --safe baseline's hello (includes the longer question)
  bravebot-anthropic-configured: prompt <n> against <m> for bravebot-anthropic hello, so skills and MCP tools add <n-m> prompt tokens and ...
```

The `configured_hello` question is longer than the `hello` question, so the difference includes a
few dozen tokens of question. The line says so when the configured run is not larger than the `--safe`
run, which means the configuration was not loaded. `configured_hello` is solved only when the reply
states how many skills the overview counts and names a tool only the stand-in server has.

The contestant's `args` has to say where its credentials come from, since the fixture directory has
none. `bravebot-anthropic-configured` in the example file reads a settings file named by
`BENCH_ANTHROPIC_SETTINGS`, with a provider block that names the key variable:

```json
{"provider": {"anthropic": {"env": ["ANTHROPIC_API_KEY"]}}}
```

`skill_load` has two turns. The first asks the model to load a skill, and the second asks about a
value that is only in the skill, so a model that did not load it cannot answer either. The columns of
the second turn show whether the load left the cached prefix as it was: a `cache_write` close to
the size of the skill on turn 2 means the loaded text moved the prefix, and a `cache_read` that
covers the loaded text means it did not. `skill_inline` runs the same turns with the two values in the
prompts, so `loading adds` in the report is the first turn's prompt with the load minus the prompt
without it, and also shows the extra requests the load costs. bravebot reports its calls, and a turn
that has to load the skill fails when no `load_skill` call for it was made, so a contestant cannot win
by guessing. The other tools have no such skills or MCP servers to load, so they do not run any of the
three scenarios and have no figure for them. `configured_hello`, `skill_load`, `skill_inline` and their
prompts have not been run against a live model.

### Goal runs and manifest runs

`goal_copy` and `manifest_copy` give bravebot the task of `plain_copy` with the same fixture and the same
prompt, so the three differ only in how the run is made. `--goal` makes the run check a condition
after each round and send the work back until it holds. `--mode manifest` plans every step before
reading anything and then runs the plan. The report prints each against `plain_copy` of the same
contestant:

```
goal and manifest runs against the same task as a plain turn (plain_copy)
  bravebot-anthropic: goal_copy prompt <n> against <m>, cost <n> against <m> (<difference>), requests ...
```

The scenarios add `--goal <condition>` or `--mode manifest` themselves, and the plan question of a
manifest run is answered by the `--dangerously-skip-permissions` that every bravebot run has
(manifest.md MANIFEST-10). `--incognito` is kept on both, since neither mode refuses it
(`manifest_conflict` and the limit checks in `crates/cli/src/invocation.rs` refuse `--agent`,
`--advisor`, `--output-schema`, `--tools`, `--system-prompt`, `--append-system-prompt`, `--resume`,
`--continue`, `--max-rounds` and `--max-tokens`). A contestant whose `args` has one of those is not run
on the two scenarios and still runs `plain_copy`. A goal run and a manifest run write the same `--json`
object as a plain turn, built from the same call list in `crates/cli/src/main.rs`, with `steps` and
`tokens` covering every round and check of a goal run. A manifest run names no session. Its calls are
the plan's steps, and its `steps` is the plan's length.

The check reads the workspace: `copy.txt` has to hold the content of `source.txt`, and `source.txt`
has to be unchanged, whatever the reply says. The other tools have no goal or manifest mode, so
they run `plain_copy` only, and their figure is the baseline. They are not scored as failures on the
other two. The goal run adds the judge's checks to the cost, and a manifest run plans in two
requests, so compare them with the plain turn by the lines above. These
scenarios and their prompts have not been run against a live model.

### Multi-file edits

`refactor_rename` asks for a function to be renamed in its definition, in three callers and in a
notes file, with a test script that already uses the new name. The check runs `test_shop.py` and
requires that it was not edited, that no file still names the old function and that every file now
names the new one, so an alias that keeps the old name fails.

The report prints, for each contestant, how many calls of each tool the turn made and how many
`read_file` calls named a file that an earlier call had already read:

```
refactor_rename: calls per tool (median over repeats), so edit_file against write_file shows how the edits were sent, ...
  bravebot-anthropic     edit_file <n>, read_file <n>, run <n>; reads of a file already read <n>
```

The `output`, `tool_calls` and `requests` columns show the rest of how the edits were sent: a tool
that rewrites whole files with `write_file` has far more `output` than one that sends `edit_file`
edits. Only bravebot reports its calls with their targets. The other tools show their call count and
say that per-tool counts and re-reads are not shown, and they are judged on the result of the test
script alone. This scenario and its prompt have not been run against a live model.

## Comparing models and routes

A contestant is a tool and a model, so the same tool can appear several times. Sonnet 5.5 on
Bedrock against the Anthropic gateway or OpenRouter is a few entries:

```json
{"contestants": [
  {"name": "bravebot-bedrock",    "tool": "bravebot", "model": "${BENCH_BEDROCK_MODEL}",
   "args": ["--safe", "--settings", "${BENCH_BRAVEBOT_SETTINGS}"]},
  {"name": "bravebot-anthropic",  "tool": "bravebot", "model": "anthropic/claude-sonnet-5-5", "args": ["--safe"]},
  {"name": "opencode-bedrock",    "tool": "opencode", "model": "amazon-bedrock/${BENCH_BEDROCK_MODEL}",
   "env": {"OPENCODE_CONFIG": "${BENCH_OPENCODE_CONFIG}"}},
  {"name": "claude-bedrock",      "tool": "claude",   "args": ["--bare"]},
  {"name": "ref-openrouter",      "tool": "reference", "model": "anthropic/claude-sonnet-4.5"}
]}
```

Fields: `name`, `tool` (`bravebot`, `claude`, `codex`, `opencode`, `reference`), `model` (passed as
`--model`, or `-m` for opencode and codex), `args` (extra flags), `env` (extra environment), `bin`,
`timeout`, `weights`. `${VAR}` is expanded in `model`, `args` and `env`, and `${BENCH_DIR}` is this
directory. The `reference` tool also takes `base_url` and `key_env`.

For a fair comparison on a model, give every contestant the same underlying model and route.
`model` for bravebot is `anthropic/<model>` for the Anthropic gateway, `openrouter/<model>` for
OpenRouter, a Brave roster name, or a Bedrock inference-profile ARN. A bravebot run with no `model`
uses whatever `/model` chose, which is not necessarily the route the other contestants use. Claude
Code takes its route from its own settings, and `bench.py` does not change it. Check each result's
reply model (bravebot `--json` has a `model` field) before comparing the numbers.

### Bedrock

An inference-profile ARN holds an account id, so it is set in the environment and in two settings
files that stay outside the repository:

- `BENCH_BEDROCK_MODEL` is the profile ARN of the model to compare. `aws bedrock
  get-inference-profile` names the model behind a profile, and the profile has to be the same model
  for every contestant. A profile for a different model measures the model.
- `BENCH_BRAVEBOT_SETTINGS` is a bravebot settings file that names the profile under
  `provider.amazon-bedrock.models`, so bravebot offers it, with the region and profile in
  `options` (see the bravebot Bedrock page).
- `BENCH_OPENCODE_CONFIG` is an opencode config with the same block, which is the format bravebot's
  provider block copies:

```json
{"provider": {"amazon-bedrock": {
  "options": {"region": "us-west-2", "profile": "<aws profile>"},
  "models": {"<profile ARN>": {"name": "bench", "limit": {"context": 200000, "output": 8192}}}}}}
```

opencode signs with the AWS profile, so a current `aws sso login` for that profile is needed (set
`AWS_PROFILE` to its name). An expired session ends the first turn with a `provider.auth` error
that the benchmark records.

## Fairness

The benchmark controls these, and each can change a result:

- **Working directory.** The same path is used on every run. Tools put the path in the system
  prompt, and a path that changed would change the cached prefix.
- **Configuration loaded.** `--safe` (bravebot) and `--bare` (Claude Code) skip user instructions,
  skills, hooks and MCP servers, so a result reflects the tool and not one machine's setup. The
  scenarios in [Configuration, MCP tools and skills](#configuration-mcp-tools-and-skills) run
  bravebot without `--safe`, in a fixture state directory, and the report gives the difference from
  the `--safe` figure. That is the standard way to measure a person's setup. Claude Code, Codex and
  opencode have no such contestant.
- **Warm-up.** Each contestant makes one unscored call first so the static prefix is already
  cached for the others to read.
- **Permissions.** Tools run with permission checks off, in a temporary directory.

## Known limits

- Within one `bravebot -p` invocation and across invocations, bravebot reads about 99% of its
  prompt from the cache on Sonnet 5.5 (Bedrock, the Anthropic API, OpenRouter and Brave). Before
  the session's scratch path moved out of the system prompt (#4513) a new process wrote the whole
  prefix again, so results from builds older than that commit are not comparable.
- Routes differ in cost on multi-turn tool conversations. Brave's gateway and OpenRouter take the
  OpenAI-compatible request body, and `multi_turn` costs about 30% more there than on Bedrock or
  the Anthropic API, which take native bodies. The other scenarios agree within 1%. The measurement
  and what was tried are in #4524.
- A route that needs an account (Brave's premium gateway) fails every scenario with HTTP 401 until
  the account is set up. The benchmark records that as a failed turn.
- Brave names models by its own keys. Its roster lists Sonnet 5.5 as `claude-3-sonnet`, with access
  `premium`, and the free endpoint's `/v1/models` lists the keys without a credential.
- Cache lifetime is about five minutes on Anthropic models, so a run that spends longer between
  turns than that measures a cold cache.
- Codex does not report cache writes for these models, so its write cost is zero.
- opencode runs through a private server (`--standalone`) and loads its own tool set and system
  prompt, so its fixed cost reflects those defaults. It has no flag for extra system text, so the
  system-prompt scenarios give it an `AGENTS.md` in the working directory, which it reads as
  standing instructions.
- Three repeats is the default. Cache behaviour on a shared endpoint is noisy, so raise `--repeats`
  before drawing a conclusion from a small gap.
- Costs are list-price ratios, not invoices. Claude Code also reports `total_cost_usd`, which is
  recorded as `cost_usd` for comparison.
