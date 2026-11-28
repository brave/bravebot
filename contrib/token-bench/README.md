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
reads 0.10, cache writes 1.25 (Anthropic) or 1.0 (OpenAI), and output 5 or 8. Anthropic bills a
one-hour cache write at 2x base while the bench weights every write at 1.25x, so a contestant that
writes one-hour entries is undercounted by the difference. A contestant can
choose a profile or state its own numbers with `"weights": {"write": 1.25, "read": 0.1,
"output": 5}`. These are list-price ratios, so change them if the prices you pay differ.

For each scenario, the cheapest contestant that solved it in every repeat scores 100, and the others
score `100 * best / cost`. A contestant that failed any repeat is scaled down by its success rate,
so a cheap wrong answer cannot win. A contestant's score is the mean over scenarios, and each
category (overhead, tools, cache, delegates, compaction, output, resume, skills, configured, modes,
edits, retries, system_prompt) has its own mean. Costs are medians over repeats.

The three `retries` scenarios are in their category's mean and are left out of the overall score. They
run only for a contestant with `fake_endpoint`, against a model that answers with a fixed key, so the
cost that ranks them is mostly the tool's own fixed prompt, which is far larger for bravebot than for
the reference loop. Counting them would move the standings by that prompt, which says nothing about
what a retry did. A contestant that ran only retry scenarios has no overall score, shows `nan`, and ranks last.
Read retries from the retry cost lines.

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
| `delegate_nested` | delegates | one turn that starts a delegate, which starts a delegate of its own to read a file |
| `compaction_recall` | compaction | four turns that each read a file of about 19,000 characters under a context budget the conversation passes, then a turn that asks for a value only the first file held |
| `large_output` | output | finding one value in a log of 2,600 lines whose `key:` line is past the first page of a read |
| `resume_warm` | resume | two turns over one conversation with no wait between them |
| `resume_after_gap` | resume | the turns of `resume_warm` with a wait longer than the prompt cache lifetime before the second |
| `configured_hello` | configured | one trivial turn with eight skills and eight MCP tool schemas loaded, without `--safe` |
| `skill_load` | skills | a turn that calls `load_skill` for a skill of about five thousand characters, then a turn about its text |
| `skill_inline` | skills | the turns of `skill_load` with the values in the prompt and no skill loaded |
| `plain_copy` | modes | copying one file to another in a plain turn, checked on the files |
| `goal_copy` | modes | the task of `plain_copy` as a `bravebot --goal` run |
| `manifest_copy` | modes | the task of `plain_copy` as a `bravebot --mode manifest` run |
| `refactor_rename` | edits | renaming a function across four modules and a notes file, checked by running the project's test script |
| `retry_baseline` | retries | reading a file and answering from it against the bench's own endpoint, with no failure |
| `retry_429` | retries | the task of `retry_baseline`, with the endpoint answering the answering request with HTTP 429 once |
| `retry_stream` | retries | the task of `retry_baseline`, with the endpoint closing the connection part way through the answering reply once |

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
the `delegates` category across tools only after checking that, and that holds for `delegate_nested`
as much as for the other two. A contestant is a tool and a model
route, so the delegate scenarios run once per route, and the `delegates` figure of each contestant
shows what delegates cost on that route.

`delegate_nested` asks for a delegate that starts a delegate of its own, and only the inner one is told
to read `deep.txt`. bravebot's `--json` `calls` lists the `spawn_agent` calls of the nested delegates as
well as the turn's: the reporter of the command line records every call it is told of and does not look
at whose it is (`crates/cli/src/progress.rs` `tool_finished`), and the handle a delegate reports through
forwards each call to the run above it (`crates/agent/src/shared.rs`, the `reports!` macro, and
`delegation.md` DELEGATE-13 for the same path of the trail). So the turn is solved for bravebot only
when at least two `spawn_agent` calls were made and the reply has the key. Nesting rests on the
prompt wording. Two sibling delegates also make two calls, and a call in `--json` carries no depth, so
the floor counts delegates only and the check does not prove that one delegate started another. The
reading of the source above is the only support for the nested calls being listed: no sample of the
real output with a nested delegate was captured here, and the scenario has not been run against a live
model. A tool that does not report calls is judged on the reply, which only the
inner delegate's file holds.

### Compaction, large output and resume after the cache expired

These three measure what the `multi_turn` scenario does not: a conversation that is shortened, a tool
result larger than one page, and a turn that finds the prompt cache expired.

`compaction_recall` runs only for bravebot. Its four first turns each read one of four files of 451
lines and reply with the line count, which only a read gives. The scenario sets
`BRAVEBOT_CONTEXT_BUDGET` to 20,000 tokens for its commands, a budget a set value keeps over the one the
endpoint advertises (COMPACT-9). The budget is meant to be passed while the files are read, so that a
compaction is made once a cut that gives up more than it keeps exists (COMPACT-8, COMPACT-5). The
figure is a choice and was not tried against a live model, so whether the run compacts is not known. The
last turn asks for the value on the `key:` line of `big_1.txt`, which no reply holds, so the answer
comes from a summary that kept it or from reading the file again, and a tool that lost it fails. The
cache columns of the last turn show what the compaction cost (COMPACT-12). `bravebot --json` does not
report a compaction, so the run does not check that one happened. A tool that never compacted and
kept the whole conversation passes too, and its prompt column is what tells the two apart. No issue
tracks adding the compaction to `--json` yet.

`large_output` has `large.log` of 2,600 lines with the `key:` line at line 2,400. A read returns 500
lines and at most 100,000 characters (READ-2), so the line is past the first page, and only a tool that
pages on, searches or runs a program gets the value. The check is the reply, and the key is nowhere
else. It runs for every tool.

`resume_after_gap` and `resume_warm` have the same two turns over one conversation, a read of
`notes.txt` and a question about one of its values. Before the second turn, `resume_after_gap` waits
`resume_gap` seconds, 330 unless the contestant sets another, which is more than the five minutes an
Anthropic prompt cache holds, so the second request finds the cache expired. The wait is a setting and
is made through one function that the selftest replaces, so the selftest does not wait. The wait is
recorded as `gap_s` on the turn that followed it and is not in that turn's wall time. The report
prints the second turn of each against the other:

```
resume after the cache expired: turn 2 of resume_after_gap against turn 2 of resume_warm (the wait is not in the wall time)
  bravebot-anthropic: turn 2 after a 330 s wait against none: cache read <n> against <m>, cache write ...
```

A route whose cache lasts longer than the wait shows no difference. Codex does not report cache writes,
so its write figure stays 0. These three scenarios and their prompts have not been run against a live
model, and `compaction_recall` has not been seen to compact.

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

### Retried and interrupted requests

A retry after an HTTP 429 or after a reply that was cut off sends the whole prompt again, and a tool
that resends more than it has to pays for it. `retry_baseline`, `retry_429` and `retry_stream` measure
that. They have one fixture (`notes.txt`, with one `key:` line among filler) and one prompt that asks
for the value on that line, so the three differ only in what the endpoint does.

The contestant talks to an endpoint that `bench.py` starts for the run, on loopback. No real
service is called. The endpoint's model asks to read `notes.txt` (chat request 1) and answers with the key it
finds in the tool result of the next request (chat request 2). It fails chat request 2 once:

| Scenario | What the endpoint does to request 2 |
|---|---|
| `retry_baseline` | answers it |
| `retry_429` | answers HTTP 429 with `Retry-After: 1`, then answers the request when it is sent again |
| `retry_stream` | sends the status line, the headers and the start of the reply, then closes the connection before the reply ends, then answers the request when it is sent again |

Only the first failure is injected, so a tool that sends the request a third time gets an answer. The
endpoint counts the chat requests it receives and the bytes of their bodies, and notes whether the
request after the failure was byte for byte the one that failed. A run is solved only when the reply
is the key, the failure was injected (for the failing scenarios) or was not (for the baseline), and the
tool sent a request after the failed one. A tool that answers from the file without asking the
endpoint is not solved, and a failing run that the endpoint never failed is not solved.

**What retry cost is.** The report prints, for each contestant, each failing scenario against its
baseline, as medians over the repeats:

```
retry cost: a run with one request failed once, minus the run with no failure (extra requests and prompt bytes are counted by the fake endpoint)
  bravebot-fake-endpoint: retry_429 extra requests +<n> (<a> against <b>), extra prompt bytes seen by the endpoint +<n> (...), extra cost +<n> units, extra wall +<s> s, failure injected in <k> of <m> runs, request sent again unchanged in <j>, solved <p>%
```

- **Extra requests** are the chat requests the endpoint received beyond the baseline's. This is the
  count that a retry adds, taken from the endpoint. What the tool reports is not used for it.
- **Extra prompt bytes** are the request body bytes the endpoint received beyond the baseline's. This
  is the prompt sent again, and it is the figure that tells a tool that resends the whole conversation
  from one that does not. Bytes stand in for tokens because the endpoint reports no cache and a
  failed request reports no usage.
- **Extra cost** is the difference of the cost units of the usage the tool reported. A 429 and a cut
  reply bill nothing, so this is expected to be near zero for both, and the endpoint's usage is the size
  of the request in bytes over four. The figure is there to show whether a tool bills the failed
  attempt, which it would if it counted the request it sent.
- **Extra wall** is the difference in seconds, which is the time the tool waited before it sent the
  request again.

The cost units in the ranking are the whole run's, so they include the tool's fixed prompt, which is
far larger for bravebot than for the reference loop. Compare the tools by the retry cost lines.

**Which tools run it.** Only a contestant with `"fake_endpoint": true` runs these scenarios, and it
runs nothing else. Two tools can have it:

- **bravebot** gets a state directory under the temporary directory (`BRAVEBOT_HOME`) with a
  `settings.json` holding a provider entry named `bench` with `options.baseURL` set to the endpoint and
  no credential, `--safe`, and the model `bench/bench-model`. `BRAVE_AI_CHAT_ENDPOINT` points at the
  same address with dummy Brave credentials, so no request leaves the machine. The gateway client speaks
  streamed chat completions, so the endpoint answers a request with `stream: true` as server-sent frames
  ending in `[DONE]`, with the usage on the frame that carries `finish_reason`.
- **reference** gets the endpoint as its `base_url`. It sends requests that are not streamed, so the
  endpoint answers those as one JSON body.

Claude Code, Codex and opencode cannot be pointed at an endpoint of the bench's own, since each takes
its route from its own settings. They are left out of these scenarios: `run` prints `left out: <name>
on <scenario>` for each, they have no row, and they are not counted as failed. A config that gives
them `fake_endpoint` is refused when it is read.

**What bravebot does.** A 429 that carries `Retry-After: 1` is sent again after a wait of that second,
and the endpoint sees one more request (`crates/aichat/src/lib.rs:1239` `ATTEMPTS`, 3 requests in all;
`:1627` `pause_after_failure`, the wait asked for or the doubling from one second, whichever is longer;
`crates/aichat/src/call.rs:385` `drive` runs the retry and `:437` waits). A 429 with no `Retry-After`
is on a longer schedule (6 requests, the first wait 8 seconds, `lib.rs:1250`, BACKEND-54), so the
endpoint always sends the header and the run does not wait 8 seconds. A reply cut off after it began
is `ChatError::Incomplete` (`lib.rs:1026` for a failed read, `:1100` for a stream that ended with no
finish frame), is sent again from the start (`lib.rs:1543`, held to the same count, BACKEND-55), and
what had arrived is dropped. The failed attempt's usage is added to the call's total only if the
service reported it, which the cut reply does not. `maxRetries` in the provider entry changes the
count, and `bench.py` leaves it unstated.

**What the reference loop does.** It sends a request again after a transport failure, a body that is
not JSON or the statuses 408, 429, 500, 502, 503 and 504, at most `retries` times (default 2, so three
requests in all, as bravebot's count). The wait before retry n is `retry_wait` seconds (default 1)
doubled for each retry after the first and held at a minute, or the whole seconds a 429's `Retry-After`
asked for if that is longer. A wait asked for beyond two minutes is not taken and the request fails.
Any other status ends the request. `retries` and `retry_wait` are contestant fields, and the wait is
made through one function that the selftest replaces, so the selftest does not sleep. This applies to
every scenario of the reference contestant, and for a run in which no request fails it sends the same
requests as a loop with no retry.

The scenarios were run once, offline, against a bravebot binary that was already built (`bravebot
0.23.0 (afa31023c)`, not built from this branch) and the reference loop, with no credentials. That run
is not an artifact of this branch and the selftest does not repeat it. The selftest covers the same
behaviour with stand-ins for the tools and the endpoint. No figure from that run is recorded here. The
scenarios have not been run against a live model or service.

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
`timeout`, `weights`, `versus` (see below) and `resume_gap` (seconds `resume_after_gap` waits, default
330). `${VAR}` is expanded in `model`, `args` and `env`, and `${BENCH_DIR}` is this
directory. The `reference` tool also takes `base_url`, `key_env`, `retries` and `retry_wait`. A
bravebot or reference contestant with `fake_endpoint: true` runs the retry scenarios and only those.

For a fair comparison on a model, give every contestant the same underlying model and route.
`model` for bravebot is `anthropic/<model>` for the Anthropic gateway, `openrouter/<model>` for
OpenRouter, a Brave roster name, or a Bedrock inference-profile ARN. A bravebot run with no `model`
uses whatever `/model` chose, which is not necessarily the route the other contestants use. Claude
Code takes its route from its own settings, and `bench.py` does not change it. Check each result's
reply model (bravebot `--json` has a `model` field) before comparing the numbers.

### Comparing two routes

A contestant with `"versus": "<other contestant>"` is compared with that contestant by the report. In
`contestants.example.json`, `bravebot-brave` and `bravebot-openrouter`, both OpenAI-compatible routes,
name `bravebot-anthropic`, a native route, so a run with those three contestants prints:

```
route comparison: the median cost of a contestant over its `versus` contestant, and each delegate scenario's gap against the multi_turn gap (#4524)
  bravebot-brave against bravebot-anthropic: cost of multi_turn +<n>%, delegate_fanout +<n>%, delegate_serial +<n>%, delegate_nested +<n>%
  bravebot-anthropic against bravebot-brave: cost of multi_turn <n>%, delegate_fanout <n>%, ...
  bravebot-brave on delegate_fanout: the gap, +<n>%, is larger than | smaller than | about the same as the multi_turn gap, +<n>%
```

Each pair is printed in both directions, since a route that costs 30% more than the other is the
other costing 23% less. A delegate scenario's gap is called about the same as the `multi_turn` gap when
the two are within 5 percentage points of each other, and larger or smaller otherwise. A scenario that
either route did not solve every time is marked, because its cost is then of a run that did not do the
work. The `versus` name has to be another contestant in the same file, which `load_config` checks. The
contestants of a pair need the same model on different routes, or the gap measures the model.

To measure delegates on two routes, give the pair credentials and run
`bench.py run --config my.json --only bravebot-anthropic,bravebot-brave --scenarios multi_turn,delegate_fanout,delegate_serial,delegate_nested --repeats 5`
then read the route comparison lines of its output, or of `bench.py report` on the results file. If the
`multi_turn` gap is not about +30%, the route has changed since #4524 and the delegate gaps are read
against the new figure, not against 30%.

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
- Whether delegates show the same route difference is not measured. Measuring it needs a
  credential for each route, which the change that added the scenarios did not have, so the
  comparison of `delegate_fanout`, `delegate_serial` and `delegate_nested` with the 30% of `multi_turn`
  is open and #4603 tracks it. It is not known whether the gap for delegates is the same, larger or smaller. The
  config and the report for it are in place (see [Comparing two routes](#comparing-two-routes)). To
  measure it, run `multi_turn` and the three delegate scenarios on a native route and an
  OpenAI-compatible route of the same model, at least five repeats, and read the route comparison
  lines: each delegate scenario is reported as larger than, smaller than or about the same as the
  `multi_turn` gap of the same pair. Compare the delegate figures across tools only after checking that
  each tool's totals include its sub-agents.
- A route that needs an account (Brave's premium gateway) fails every scenario with HTTP 401 until
  the account is set up. The benchmark records that as a failed turn.
- Brave names models by its own keys. Its roster lists Sonnet 5.5 as `claude-3-sonnet`, with access
  `premium`, and the free endpoint's `/v1/models` lists the keys without a credential.
- Cache lifetime is about five minutes on Anthropic models, so a run that spends longer between
  turns than that measures a cold cache. `resume_after_gap` does that on purpose, with a wait of
  `resume_gap` seconds.
- Codex does not report cache writes for these models, so its write cost is zero.
- opencode runs through a private server (`--standalone`) and loads its own tool set and system
  prompt, so its fixed cost reflects those defaults. It has no flag for extra system text, so the
  system-prompt scenarios give it an `AGENTS.md` in the working directory, which it reads as
  standing instructions.
- Three repeats is the default. Cache behaviour on a shared endpoint is noisy, so raise `--repeats`
  before drawing a conclusion from a small gap.
- Costs are list-price ratios, not invoices. Claude Code also reports `total_cost_usd`, which is
  recorded as `cost_usd` for comparison.
